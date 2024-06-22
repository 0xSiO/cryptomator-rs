use std::{
    fs::{self, File},
    io::{Cursor, Read, Seek, Write},
    path::PathBuf,
    str::FromStr,
};

use base64ct::{Base64, Encoding};
use cryptomator::{
    crypto::FileCryptor, io::EncryptedStream, util, CipherCombo, MasterKey, Vault, VaultConfig,
};
use jsonwebtoken::{TokenData, Validation};
use uuid::Uuid;

#[test]
pub fn siv_gcm_basic() {
    // Check vault import
    let vault = Vault::open(
        "tests/fixtures/vault_v8_siv_gcm/vault.cryptomator",
        String::from("password"),
    )
    .unwrap();

    assert_eq!(
        vault.path(),
        fs::canonicalize("./tests/fixtures/vault_v8_siv_gcm").unwrap()
    );

    assert_eq!(
        vault.config().claims,
        VaultConfig {
            jti: Uuid::from_str("46cd26de-d575-4563-ad30-432123f85f36").unwrap(),
            format: 8,
            shortening_threshold: 220,
            cipher_combo: CipherCombo::SivGcm
        }
    );

    // Check key import
    let key = vault.master_key();

    assert_eq!(*key, unsafe {
        MasterKey::from_bytes(
            Base64::decode_vec("sXs8e6rKQX3iySTUkOd6V0FqaM3nqN/x8ULcUYdtBXQBSSDBbf8FEBAkUuGhpqot8leMQTfevZKICb7t8voIOQ==")
                .unwrap()
                .try_into()
                .unwrap(),
        )
    });

    // Check JWT signing/verifying
    let config_jwt =
        util::sign_jwt(vault.config().header.clone(), vault.config().claims, key).unwrap();

    let mut validation = Validation::new(vault.config().header.alg);
    validation.validate_exp = false;
    validation.required_spec_claims.clear();
    let decoded_config: TokenData<VaultConfig> =
        util::verify_jwt(config_jwt, validation, key).unwrap();

    assert_eq!(decoded_config.header, vault.config().header);
    assert_eq!(decoded_config.claims, vault.config().claims);

    // Check file name encryption/decryption
    let cryptor = vault.cryptor();

    assert_eq!(
        cryptor
            .decrypt_name("AlBBrYyQQqFiMXocarsNhcWd2oQ0yyRu86LZdZw=", "")
            .unwrap(),
        "test_file.txt"
    );
    assert_eq!(
        cryptor.encrypt_name("test_file.txt", "").unwrap(),
        "AlBBrYyQQqFiMXocarsNhcWd2oQ0yyRu86LZdZw="
    );

    assert_eq!(
        cryptor
            .decrypt_name(
                "j2O1bILonFELjBCQTaqZEBgfUh1_uHvXjOdMdc2ZEg==",
                "1a3534ba-34fb-4ba6-ad67-1e37627d40be"
            )
            .unwrap(),
        "test_file_2.txt"
    );
    assert_eq!(
        cryptor
            .encrypt_name("test_file_2.txt", "1a3534ba-34fb-4ba6-ad67-1e37627d40be")
            .unwrap(),
        "j2O1bILonFELjBCQTaqZEBgfUh1_uHvXjOdMdc2ZEg=="
    );

    // Check file header encryption/decryption
    let ciphertext = std::fs::read(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/AlBBrYyQQqFiMXocarsNhcWd2oQ0yyRu86LZdZw=.c9r",
    )
    .unwrap();

    assert_eq!(
        Base64::encode_string(&ciphertext),
        "EOc16Sc/NMUcA9N8K6aYhNWdXdX34sZbTUw0WWVXjtxDAHiuLoTtrre0PNzb1SwvLGz2Ow6/7lBDb+inNxZr7sAc5BwkJHmHJaEjLbOU5i+tCSI7inkX9YmFv6Zm9ZjeDy8lK1360cCTHQ9d4IQ2dhX6Qa5ZMeKSC31r5Y3Eg+rY0U8eIjzby8Q="
    );

    let header = cryptor.decrypt_header(&ciphertext[..68]).unwrap();
    assert_eq!(cryptor.encrypt_header(&header).unwrap(), &ciphertext[..68]);

    // Check file content decryption
    assert_eq!(
        cryptor
            .decrypt_chunk(&ciphertext[68..], &header, 0)
            .unwrap(),
        b"this is a test file with some text in it\n"
    );

    // Check root directory ID hashing
    let ciphertext = std::fs::read(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/dirid.c9r",
    )
    .unwrap();

    assert_eq!(
        Base64::encode_string(&ciphertext),
        "ftvD3GxyBnhbFU6kxs5CEHUh0LMhCXHVfQJLZrVCbq9cZ8ptPl2KD9oEGvGlcaI/XVhPT17C4y1P9Y6qhDTRZFGF5xw="
    );

    let _ = cryptor.decrypt_header(&ciphertext[..68]).unwrap();
    assert_eq!(&ciphertext[68..], b"");

    assert_eq!(
        cryptor.hash_dir_id("").unwrap(),
        PathBuf::from("RC").join("WG5EI3VR4DOIGAFUPFXLALP5SBGCL5")
    );

    // Check subdirectory ID hashing
    let ciphertext = std::fs::read(
        "tests/fixtures/vault_v8_siv_gcm/d/RT/C3KT7DD5C3X6QE32X4IL6PM6WHHNB5/dirid.c9r",
    )
    .unwrap();

    assert_eq!(
        Base64::encode_string(&ciphertext),
        "u53iAARXqaZVGLJMguDq2KQZ2A5eu/jxzjsqapLwVGEe2FeazwpWRptqUHEFzKVwbFh16L7Y++pR1s+WunbJU0uOKvVS9lDTEV9B9MsjSOIijAsUg0tgcoOplb5BL5Og0G10AgbWCall/smk1fgsWaIF0y2g8ScFhlmMJGA7HhXqolOf"
    );

    let header = cryptor.decrypt_header(&ciphertext[..68]).unwrap();

    assert_eq!(
        cryptor
            .decrypt_chunk(&ciphertext[68..], &header, 0)
            .unwrap(),
        b"1a3534ba-34fb-4ba6-ad67-1e37627d40be"
    );

    assert_eq!(
        cryptor
            .hash_dir_id("1a3534ba-34fb-4ba6-ad67-1e37627d40be")
            .unwrap(),
        PathBuf::from("RT").join("C3KT7DD5C3X6QE32X4IL6PM6WHHNB5")
    );

    // Check reading smaller files
    let file = File::open(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/AlBBrYyQQqFiMXocarsNhcWd2oQ0yyRu86LZdZw=.c9r",
    )
    .unwrap();
    let mut stream = EncryptedStream::open(cryptor, file).unwrap();

    let mut decrypted = String::new();
    stream.read_to_string(&mut decrypted).unwrap();
    assert_eq!(decrypted, "this is a test file with some text in it\n");

    // Check reading larger files
    let file = File::open(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/LNyfONa3J2M1pirw-S-YBasDwUyV7RyhSwz7oMlP.c9r",
    )
    .unwrap();
    let mut stream = EncryptedStream::open(cryptor, file).unwrap();

    let mut decrypted = Vec::new();
    stream.read_to_end(&mut decrypted).unwrap();
    assert_eq!(
        decrypted,
        fs::read("tests/fixtures/test_image.jpg").unwrap()
    );

    // Check writing smaller files
    let ciphertext = std::fs::read(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/AlBBrYyQQqFiMXocarsNhcWd2oQ0yyRu86LZdZw=.c9r",
    )
    .unwrap();

    let mut buffer = Vec::new();
    let mut stream = EncryptedStream::open(cryptor, Cursor::new(&mut buffer)).unwrap();
    stream
        .write_all(b"this is a test file with some text in it\n")
        .unwrap();
    stream.flush().unwrap();

    let mut decrypted = String::new();
    stream.rewind().unwrap();
    stream.read_to_string(&mut decrypted).unwrap();

    assert_eq!(buffer.len(), ciphertext.len());
    assert_eq!(decrypted, "this is a test file with some text in it\n");

    // Check writing larger files
    let ciphertext = std::fs::read(
        "tests/fixtures/vault_v8_siv_gcm/d/RC/WG5EI3VR4DOIGAFUPFXLALP5SBGCL5/LNyfONa3J2M1pirw-S-YBasDwUyV7RyhSwz7oMlP.c9r",
    )
    .unwrap();
    let image_data = fs::read("tests/fixtures/test_image.jpg").unwrap();

    let mut buffer = Vec::new();
    let mut stream = EncryptedStream::open(cryptor, Cursor::new(&mut buffer)).unwrap();
    stream.write_all(&image_data).unwrap();
    stream.flush().unwrap();

    let mut decrypted = Vec::new();
    stream.rewind().unwrap();
    stream.read_to_end(&mut decrypted).unwrap();

    assert_eq!(buffer.len(), ciphertext.len());
    assert_eq!(decrypted, image_data);
}
