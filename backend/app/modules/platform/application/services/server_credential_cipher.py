from __future__ import annotations

import base64
import hashlib

from app.shared.infra.config.settings import Settings


class ServerCredentialCipher:
    _KDF_SALT = b"platform.server_credentials.v1"
    _KDF_ITERATIONS = 390000

    def __init__(self, secret: str) -> None:
        secret = str(secret).strip()
        if not secret:
            raise ValueError("server credential encryption secret is not configured")

        try:
            from cryptography.fernet import Fernet
        except ImportError as exc:
            raise RuntimeError("cryptography package is required for server credential encryption") from exc

        derived_key = hashlib.pbkdf2_hmac(
            "sha256",
            secret.encode("utf-8"),
            self._KDF_SALT,
            self._KDF_ITERATIONS,
            dklen=32,
        )
        self._fernet = Fernet(base64.urlsafe_b64encode(derived_key))

    @classmethod
    def from_settings(cls, settings: Settings) -> "ServerCredentialCipher":
        return cls(settings.get_server_credential_secret())

    def encrypt(self, plaintext: str) -> str:
        return self._fernet.encrypt(str(plaintext).encode("utf-8")).decode("utf-8")

    def decrypt(self, ciphertext: str) -> str:
        try:
            return self._fernet.decrypt(str(ciphertext).encode("utf-8")).decode("utf-8")
        except Exception as exc:
            raise ValueError("invalid server credential ciphertext") from exc
