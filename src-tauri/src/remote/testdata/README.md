# Test SSH keys

Keys generated with `ssh-keygen` for the in-process SSH server of the SFTP tests
(`src/remote/test_server.rs`). They protect nothing and must never be used
elsewhere.

- `host_a`, `host_b`: server host keys (ed25519).
- `client_ed25519`, `client_rsa`: authorized client keys.
- `client_encrypted`: authorized client key protected by the passphrase
  `correct horse` (2 KDF rounds, to keep tests fast).
- `stranger`: a client key the server does not authorize.
- `not_a_key`: a file that is not a key.
