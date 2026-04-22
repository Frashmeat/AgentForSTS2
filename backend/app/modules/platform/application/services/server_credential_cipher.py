from __future__ import annotations

import base64
import hashlib
import hmac
import secrets

from app.shared.infra.config.settings import Settings


class ServerCredentialCipher:
    _KDF_SALT = b"platform.server_credentials.v1"
    _KDF_ITERATIONS = 390000
    _FALLBACK_PREFIX = "fallback1:"

    def __init__(self, secret: str) -> None:
        secret = str(secret).strip()
        if not secret:
            raise ValueError("server credential encryption secret is not configured")

        self._secret_bytes = secret.encode("utf-8")
        self._fernet = None
        try:
            from cryptography.fernet import Fernet
        except ImportError:
            Fernet = None

        if Fernet is not None:
            derived_key = hashlib.pbkdf2_hmac(
                "sha256",
                self._secret_bytes,
                self._KDF_SALT,
                self._KDF_ITERATIONS,
                dklen=32,
            )
            self._fernet = Fernet(base64.urlsafe_b64encode(derived_key))

    @classmethod
    def from_settings(cls, settings: Settings) -> "ServerCredentialCipher":
        return cls(settings.get_server_credential_secret())

    def encrypt(self, plaintext: str) -> str:
        normalized = str(plaintext).encode("utf-8")
        if self._fernet is not None:
            return self._fernet.encrypt(normalized).decode("utf-8")
        nonce = secrets.token_bytes(16)
        ciphertext = self._xor_with_keystream(normalized, nonce)
        mac = hmac.new(self._secret_bytes, nonce + ciphertext, hashlib.sha256).digest()
        token = base64.urlsafe_b64encode(nonce + mac + ciphertext).decode("ascii")
        return f"{self._FALLBACK_PREFIX}{token}"

    def decrypt(self, ciphertext: str) -> str:
        raw_value = str(ciphertext)
        try:
            if raw_value.startswith(self._FALLBACK_PREFIX):
                return self._decrypt_fallback(raw_value[len(self._FALLBACK_PREFIX):])
            if self._fernet is None:
                raise ValueError("fernet backend is unavailable for this ciphertext")
            return self._fernet.decrypt(raw_value.encode("utf-8")).decode("utf-8")
        except Exception as exc:
            raise ValueError("invalid server credential ciphertext") from exc

    def _decrypt_fallback(self, token: str) -> str:
        decoded = base64.urlsafe_b64decode(token.encode("ascii"))
        if len(decoded) < 48:
            raise ValueError("fallback ciphertext is truncated")
        nonce = decoded[:16]
        mac = decoded[16:48]
        encrypted = decoded[48:]
        expected_mac = hmac.new(self._secret_bytes, nonce + encrypted, hashlib.sha256).digest()
        if not hmac.compare_digest(mac, expected_mac):
            raise ValueError("fallback ciphertext signature mismatch")
        plaintext = self._xor_with_keystream(encrypted, nonce)
        return plaintext.decode("utf-8")

    def _xor_with_keystream(self, data: bytes, nonce: bytes) -> bytes:
        stream = bytearray()
        counter = 0
        while len(stream) < len(data):
            stream.extend(
                hashlib.sha256(
                    self._secret_bytes + self._KDF_SALT + nonce + counter.to_bytes(4, "big")
                ).digest()
            )
            counter += 1
        return bytes(left ^ right for left, right in zip(data, stream))
