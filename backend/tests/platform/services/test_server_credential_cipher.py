from __future__ import annotations

from app.modules.platform.application.services.server_credential_cipher import ServerCredentialCipher


def test_server_credential_cipher_roundtrip_without_cryptography_dependency():
    cipher = ServerCredentialCipher("cipher-test-secret")

    ciphertext = cipher.encrypt("sk-live-openai")
    plaintext = cipher.decrypt(ciphertext)

    assert ciphertext
    assert plaintext == "sk-live-openai"
