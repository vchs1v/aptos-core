// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::{
    common::{
        types::{EncodingOptions, EncodingType, Error, KeyType, PromptOptions},
        utils::{append_file_extension, prompt_yes, to_common_success_result},
    },
    CliResult,
};
use aptos_crypto::{
    ed25519, ed25519::Ed25519PrivateKey, x25519, PrivateKey, Uniform, ValidCryptoMaterial,
    ValidCryptoMaterialStringExt,
};
use rand::SeedableRng;
use std::{
    fs::File,
    io::Write,
    path::{Path, PathBuf},
};
use structopt::StructOpt;

/// CLI tool for generating, inspecting, and interacting with keys.
#[derive(Debug, StructOpt)]
pub enum KeyTool {
    Generate(GenerateKey),
}

impl KeyTool {
    pub async fn execute(self) -> CliResult {
        match self {
            KeyTool::Generate(generate) => to_common_success_result(generate.execute()),
        }
    }
}

/// Generates a `x25519` or `ed25519` key.
///
/// This can be used for generating an identity.
#[derive(Debug, StructOpt)]
pub struct GenerateKey {
    /// Key type: `x25519` or `ed25519`
    #[structopt(long, default_value = "ed25519")]
    key_type: KeyType,
    #[structopt(flatten)]
    save_params: SaveKey,
}

impl GenerateKey {
    fn execute(self) -> Result<(), Error> {
        self.save_params.check_key_file()?;

        // Generate a ed25519 key
        let mut rng = rand::rngs::StdRng::from_entropy();
        let ed25519_key = Ed25519PrivateKey::generate(&mut rng);

        // Convert it to the appropriate type and save it
        match self.key_type {
            KeyType::X25519 => {
                let private_key =
                    x25519::PrivateKey::from_ed25519_private_bytes(&ed25519_key.to_bytes())
                        .map_err(|err| Error::UnexpectedError(err.to_string()))?;
                self.save_params.save_key(&private_key, "x22519")
            }
            KeyType::Ed25519 => self.save_params.save_key(&ed25519_key, "ed22519"),
        }
    }

    /// A test friendly typed key generation for x25519 keys.
    pub fn generate_x25519(
        encoding_type: EncodingType,
        key_file: &Path,
    ) -> Result<(x25519::PrivateKey, x25519::PublicKey), Error> {
        let args = format!(
            "generate --key-type {key_type:?} --key-file {key_file} --encoding {encoding_type:?} --assume-yes",
            key_type = KeyType::X25519,
            key_file = key_file.to_str().unwrap(),
            encoding_type = encoding_type,
        );
        let command = GenerateKey::from_iter(args.split_whitespace());
        command.execute()?;
        Ok((
            load_key(key_file, encoding_type)?,
            load_key(&append_file_extension(key_file, ".pub")?, encoding_type)?,
        ))
    }

    /// A test friendly typed key generation for e25519 keys.
    pub fn generate_ed25519(
        encoding_type: EncodingType,
        key_file: &Path,
    ) -> Result<(ed25519::Ed25519PrivateKey, ed25519::Ed25519PublicKey), Error> {
        let args = format!(
            "generate --key-type {key_type:?} --key-file {key_file} --encoding {encoding_type:?} --assume-yes",
            key_type = KeyType::Ed25519,
            key_file = key_file.to_str().unwrap(),
            encoding_type = encoding_type,
        );
        let command = GenerateKey::from_iter(args.split_whitespace());
        command.execute()?;
        Ok((
            load_key(key_file, encoding_type)?,
            load_key(&append_file_extension(key_file, ".pub")?, encoding_type)?,
        ))
    }
}

#[derive(Debug, StructOpt)]
pub struct SaveKey {
    /// Private key output file name.  Public key will be saved to <key-file>.pub
    #[structopt(long, parse(from_os_str))]
    key_file: PathBuf,
    #[structopt(flatten)]
    encoding_options: EncodingOptions,
    #[structopt(flatten)]
    prompt_options: PromptOptions,
}

impl SaveKey {
    /// Public key file name
    fn public_key_file(&self) -> Result<PathBuf, Error> {
        append_file_extension(&self.key_file, ".pub")
    }

    /// Check if the key file exists already
    pub fn check_key_file(&self) -> Result<(), Error> {
        // Check if file already exists
        check_if_file_exists(&self.key_file, self.prompt_options.assume_yes)?;
        check_if_file_exists(&self.public_key_file()?, self.prompt_options.assume_yes)
    }

    /// Saves a key to a file encoded in a string
    pub fn save_key<Key: PrivateKey + ValidCryptoMaterial>(
        &self,
        key: &Key,
        key_name: &'static str,
    ) -> Result<(), Error> {
        let encoded_private_key = encode_key(self.encoding_options.encoding, key, key_name)?;
        let encoded_public_key =
            encode_key(self.encoding_options.encoding, &key.public_key(), key_name)?;

        // Write private and public keys to files
        write_to_file(&self.key_file, key_name, encoded_private_key)?;
        write_to_file(&self.public_key_file()?, key_name, encoded_public_key)
    }
}

/// Encodes `Key` into one of the `EncodingType`s
pub fn encode_key<Key: ValidCryptoMaterial>(
    encoding: EncodingType,
    key: &Key,
    key_name: &str,
) -> Result<Vec<u8>, Error> {
    Ok(match encoding {
        EncodingType::Hex => hex::encode_upper(key.to_bytes()).into_bytes(),
        EncodingType::BCS => {
            bcs::to_bytes(key).map_err(|err| Error::BCS(key_name.to_string(), err))?
        }
        EncodingType::Base64 => base64::encode(key.to_bytes()).into_bytes(),
    })
}

/// Write a `Vec<u8>` to a file
fn write_to_file(key_file: &Path, key_name: &str, encoded_key: Vec<u8>) -> Result<(), Error> {
    let mut file = File::create(key_file).map_err(|e| Error::IO(key_name.to_string(), e))?;
    file.write_all(&encoded_key)
        .map_err(|e| Error::IO(key_name.to_string(), e))
}

/// Checks if a file exists, being overridden by `--assume-yes`
fn check_if_file_exists(file: &Path, assume_yes: bool) -> Result<(), Error> {
    if file.exists()
        && !assume_yes
        && !prompt_yes(
            format!(
                "{:?} already exists, are you sure you want to overwrite it?",
                file.as_os_str()
            )
            .as_str(),
        )
    {
        Err(Error::AbortedError)
    } else {
        Ok(())
    }
}

/// Loads a key to a file hex string encoded
pub fn load_key<Key: ValidCryptoMaterial>(
    path: &Path,
    encoding: EncodingType,
) -> Result<Key, Error> {
    let data = std::fs::read(&path).map_err(|err| {
        Error::UnableToReadFile(path.to_str().unwrap().to_string(), err.to_string())
    })?;

    match encoding {
        EncodingType::BCS => {
            bcs::from_bytes(&data).map_err(|err| Error::BCS("Key".to_string(), err))
        }
        EncodingType::Hex => {
            let hex_string = String::from_utf8(data).unwrap();
            Key::from_encoded_string(hex_string.trim())
                .map_err(|err| Error::UnableToParse("Key", err.to_string()))
        }
        EncodingType::Base64 => {
            let string = String::from_utf8(data).unwrap();
            let bytes = base64::decode(string.trim())
                .map_err(|err| Error::UnableToParse("Key", err.to_string()))?;
            Key::try_from(bytes.as_slice())
                .map_err(|err| Error::UnexpectedError(format!("Failed to parse key {}", err)))
        }
    }
}