//! Product-neutral deploy artifact validation helpers.

use std::{
    fs,
    io::{Cursor, Read},
    path::Path,
};

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use zip::ZipArchive;

#[derive(Debug, Error)]
pub enum DeployArtifactError {
    #[error("{0}")]
    InvalidArtifact(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum BinaryArch {
    X86_64,
    Aarch64,
}

impl BinaryArch {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeploySignatureMarker {
    pub schema_version: u8,
    pub component: String,
    pub version: String,
    pub arch: String,
    pub content_sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SignatureValidationInput<'a> {
    pub payload_version: &'a str,
    pub expected_schema_version: u8,
    pub expected_component: &'a str,
    pub expected_version: &'a str,
    pub expected_arch: &'a str,
    pub expected_content_sha256: &'a str,
    pub verify_key_hex: &'a str,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WebZipValidationConfig {
    pub marker_file: String,
    pub max_files: usize,
    pub max_single_uncompressed: u64,
    pub max_total_uncompressed: u64,
}

impl WebZipValidationConfig {
    pub fn new(marker_file: impl Into<String>) -> Self {
        Self {
            marker_file: marker_file.into(),
            max_files: 5000,
            max_single_uncompressed: 5 * 1024 * 1024,
            max_total_uncompressed: 50 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct WebZipValidationReport {
    pub marker_content: String,
    pub content_sha256: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AppendedMarkerBounds {
    pub marker_start: usize,
    pub marker_content_start: usize,
    pub marker_end: usize,
}

pub fn validate_binary_marker(
    file_data: &[u8],
    marker_prefix: &[u8],
    allow_script: bool,
) -> Result<(), DeployArtifactError> {
    let valid_type =
        is_elf(file_data) || is_macho_or_fat(file_data) || (allow_script && is_script(file_data));
    if !valid_type {
        return Err(DeployArtifactError::InvalidArtifact(
            "binary artifact must be an executable binary".to_string(),
        ));
    }

    if !file_data
        .windows(marker_prefix.len())
        .any(|window| window == marker_prefix)
    {
        return Err(DeployArtifactError::InvalidArtifact(
            "binary artifact marker check failed".to_string(),
        ));
    }

    Ok(())
}

pub fn validate_web_dist_zip(
    file_data: &[u8],
    config: &WebZipValidationConfig,
) -> Result<WebZipValidationReport, DeployArtifactError> {
    if !is_zip(file_data) {
        return Err(DeployArtifactError::InvalidArtifact(
            "web artifact must be a zip file".to_string(),
        ));
    }

    let mut archive = ZipArchive::new(Cursor::new(file_data)).map_err(|_| {
        DeployArtifactError::InvalidArtifact("web zip cannot be parsed".to_string())
    })?;
    let file_count = archive.len();
    if file_count == 0 || file_count > config.max_files {
        return Err(DeployArtifactError::InvalidArtifact(format!(
            "web zip file count is invalid: {file_count}"
        )));
    }

    let mut has_index = false;
    let mut has_asset = false;
    let mut marker_content = None;
    let mut total_uncompressed = 0_u64;
    let mut content_entries = Vec::new();

    for index in 0..file_count {
        let mut file = archive
            .by_index(index)
            .map_err(|_| DeployArtifactError::InvalidArtifact("web zip read failed".to_string()))?;
        let Some(enclosed_name) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err(DeployArtifactError::InvalidArtifact(
                "web zip contains invalid paths".to_string(),
            ));
        };
        let Some(name) = zip_path_name(&enclosed_name) else {
            return Err(DeployArtifactError::InvalidArtifact(
                "web zip contains invalid paths".to_string(),
            ));
        };

        if name == "dist/index.html" {
            has_index = true;
        }
        if name.starts_with("dist/assets/") && (name.ends_with(".js") || name.ends_with(".css")) {
            has_asset = true;
        }
        if name == config.marker_file {
            marker_content = Some(read_zip_entry_to_string(&mut file)?);
        }

        let size = file.size();
        if size > config.max_single_uncompressed {
            return Err(DeployArtifactError::InvalidArtifact(format!(
                "web zip file is too large after unzip: {name}"
            )));
        }
        total_uncompressed = total_uncompressed.saturating_add(size);
        if total_uncompressed > config.max_total_uncompressed {
            return Err(DeployArtifactError::InvalidArtifact(
                "web zip total uncompressed size is too large".to_string(),
            ));
        }

        if name != config.marker_file && file.is_file() {
            let content_hash = zip_file_content_hash(&mut file)?;
            content_entries.push((name, size, content_hash));
        }
    }

    if !has_index {
        return Err(DeployArtifactError::InvalidArtifact(
            "web zip must contain dist/index.html".to_string(),
        ));
    }
    if !has_asset {
        return Err(DeployArtifactError::InvalidArtifact(
            "web zip must contain dist/assets/*.js or *.css".to_string(),
        ));
    }
    let Some(marker_content) = marker_content else {
        return Err(DeployArtifactError::InvalidArtifact(format!(
            "web zip must contain {}",
            config.marker_file
        )));
    };

    Ok(WebZipValidationReport {
        marker_content,
        content_sha256: web_content_hash(content_entries),
    })
}

pub fn extract_zip_to_dir(zip_path: &Path, output_dir: &Path) -> Result<(), DeployArtifactError> {
    let file = fs::File::open(zip_path)?;
    let mut archive = ZipArchive::new(file).map_err(|_| {
        DeployArtifactError::InvalidArtifact("web zip cannot be parsed".to_string())
    })?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|_| DeployArtifactError::InvalidArtifact("web zip read failed".to_string()))?;
        let Some(enclosed_name) = file.enclosed_name().map(|path| path.to_path_buf()) else {
            return Err(DeployArtifactError::InvalidArtifact(
                "web zip contains invalid paths".to_string(),
            ));
        };
        let output_path = output_dir.join(enclosed_name);
        if file.is_dir() {
            fs::create_dir_all(&output_path)?;
            continue;
        }
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = fs::File::create(&output_path)?;
        std::io::copy(&mut file, &mut output)?;
    }

    Ok(())
}

pub fn validate_deploy_signature_marker(
    marker: &DeploySignatureMarker,
    input: &SignatureValidationInput<'_>,
) -> Result<(), DeployArtifactError> {
    if marker.schema_version != input.expected_schema_version {
        return Err(DeployArtifactError::InvalidArtifact(
            "deploy signature marker schema version is invalid".to_string(),
        ));
    }
    if marker.component != input.expected_component {
        return Err(DeployArtifactError::InvalidArtifact(format!(
            "deploy signature component mismatch: expected {}",
            input.expected_component
        )));
    }
    if marker.version != input.expected_version {
        return Err(DeployArtifactError::InvalidArtifact(format!(
            "deploy signature version mismatch: expected {}",
            input.expected_version
        )));
    }
    if marker.arch != input.expected_arch {
        return Err(DeployArtifactError::InvalidArtifact(format!(
            "deploy signature arch mismatch: expected {}",
            input.expected_arch
        )));
    }
    if marker.content_sha256 != input.expected_content_sha256 {
        return Err(DeployArtifactError::InvalidArtifact(
            "deploy signature content hash mismatch".to_string(),
        ));
    }

    let verifying_key = parse_verify_key(input.verify_key_hex)?;
    let signature = parse_signature(&marker.signature)?;
    let payload = deploy_signature_payload(
        input.payload_version,
        input.expected_component,
        input.expected_version,
        input.expected_arch,
        input.expected_content_sha256,
    );
    verifying_key
        .verify(payload.as_bytes(), &signature)
        .map_err(|_| {
            DeployArtifactError::InvalidArtifact("deploy signature verification failed".to_string())
        })
}

pub fn append_marker_bounds(
    file_data: &[u8],
    marker_begin: &[u8],
    marker_end: &[u8],
) -> Result<AppendedMarkerBounds, DeployArtifactError> {
    let marker_start = find_last_subslice(file_data, marker_begin).ok_or_else(|| {
        DeployArtifactError::InvalidArtifact("appended marker is required".to_string())
    })?;
    let marker_content_start = marker_start + marker_begin.len();
    let marker_end = find_subslice(&file_data[marker_content_start..], marker_end)
        .map(|offset| marker_content_start + offset)
        .ok_or_else(|| {
            DeployArtifactError::InvalidArtifact("appended marker is incomplete".to_string())
        })?;

    Ok(AppendedMarkerBounds {
        marker_start,
        marker_content_start,
        marker_end,
    })
}

pub fn deploy_signature_payload(
    payload_version: &str,
    component: &str,
    version: &str,
    arch: &str,
    content_hash: &str,
) -> String {
    format!(
        "{payload_version}\ncomponent={component}\nversion={version}\narch={arch}\ncontent_sha256={content_hash}\n"
    )
}

pub fn detect_binary_arch(file_data: &[u8]) -> Result<Option<BinaryArch>, DeployArtifactError> {
    if is_elf(file_data) {
        return detect_elf_arch(file_data);
    }
    if is_macho_or_fat(file_data) {
        return detect_macho_arch(file_data);
    }
    Ok(None)
}

pub fn normalize_binary_arch(value: &str) -> Option<&'static str> {
    binary_arch_from_str(value).map(BinaryArch::as_str)
}

pub fn binary_arch_from_str(value: &str) -> Option<BinaryArch> {
    match value.trim().to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" => Some(BinaryArch::X86_64),
        "aarch64" | "arm64" => Some(BinaryArch::Aarch64),
        _ => None,
    }
}

pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

pub fn is_zip(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == b'P' && bytes[1] == b'K'
}

pub fn is_elf(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && bytes[0] == 0x7F && bytes[1] == b'E' && bytes[2] == b'L' && bytes[3] == b'F'
}

pub fn is_script(bytes: &[u8]) -> bool {
    bytes.len() >= 2 && bytes[0] == b'#' && bytes[1] == b'!'
}

pub fn is_macho_or_fat(bytes: &[u8]) -> bool {
    if bytes.len() < 4 {
        return false;
    }
    let be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    matches!(be, 0xFEEDFACE | 0xFEEDFACF | 0xCAFEBABE | 0xBEBAFECA)
        || matches!(le, 0xFEEDFACE | 0xFEEDFACF | 0xCAFEBABE | 0xBEBAFECA)
}

pub fn zip_path_name(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(part) = component else {
            return None;
        };
        parts.push(part.to_str()?.to_string());
    }
    Some(parts.join("/"))
}

pub fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub fn find_last_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(haystack.len());
    }
    haystack
        .windows(needle.len())
        .rposition(|window| window == needle)
}

fn read_zip_entry_to_string<R: Read>(reader: &mut R) -> Result<String, DeployArtifactError> {
    let mut content = String::new();
    reader.read_to_string(&mut content).map_err(|_| {
        DeployArtifactError::InvalidArtifact("web marker cannot be read".to_string())
    })?;
    Ok(content)
}

fn zip_file_content_hash<R: Read>(reader: &mut R) -> Result<String, DeployArtifactError> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| DeployArtifactError::InvalidArtifact("web zip read failed".to_string()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(finalize_sha256_hex(hasher))
}

fn web_content_hash(mut entries: Vec<(String, u64, String)>) -> String {
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut hasher = Sha256::new();
    for (name, size, content_hash) in entries {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(size.to_le_bytes());
        hasher.update(content_hash.as_bytes());
        hasher.update([0]);
    }
    finalize_sha256_hex(hasher)
}

fn finalize_sha256_hex(hasher: Sha256) -> String {
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>()
}

fn parse_verify_key(value: &str) -> Result<VerifyingKey, DeployArtifactError> {
    let bytes = decode_hex(value).map_err(|err| {
        DeployArtifactError::InvalidArtifact(format!("deploy verify key is invalid: {err}"))
    })?;
    let key: [u8; 32] = bytes.try_into().map_err(|_| {
        DeployArtifactError::InvalidArtifact("deploy verify key must be 32 bytes".to_string())
    })?;
    VerifyingKey::from_bytes(&key).map_err(|_| {
        DeployArtifactError::InvalidArtifact("deploy verify key is invalid".to_string())
    })
}

fn parse_signature(value: &str) -> Result<Signature, DeployArtifactError> {
    let bytes = decode_hex(value).map_err(|err| {
        DeployArtifactError::InvalidArtifact(format!("deploy signature is invalid: {err}"))
    })?;
    let signature: [u8; 64] = bytes.try_into().map_err(|_| {
        DeployArtifactError::InvalidArtifact("deploy signature must be 64 bytes".to_string())
    })?;
    Ok(Signature::from_bytes(&signature))
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    let value = value.trim();
    if !value.len().is_multiple_of(2) {
        return Err("hex length must be even".to_string());
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    let chars = value.as_bytes();
    for index in (0..chars.len()).step_by(2) {
        let high = hex_value(chars[index])?;
        let low = hex_value(chars[index + 1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_value(value: u8) -> Result<u8, String> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err("invalid hex character".to_string()),
    }
}

fn detect_elf_arch(bytes: &[u8]) -> Result<Option<BinaryArch>, DeployArtifactError> {
    if bytes.len() < 24 {
        return Ok(None);
    }
    let endian = bytes[5];
    let machine = match endian {
        1 => u16::from_le_bytes([bytes[18], bytes[19]]),
        2 => u16::from_be_bytes([bytes[18], bytes[19]]),
        _ => return Ok(None),
    };
    match machine {
        62 => Ok(Some(BinaryArch::X86_64)),
        183 => Ok(Some(BinaryArch::Aarch64)),
        _ => Ok(None),
    }
}

fn detect_macho_arch(bytes: &[u8]) -> Result<Option<BinaryArch>, DeployArtifactError> {
    if bytes.len() < 8 {
        return Ok(None);
    }
    let magic_be = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let magic_le = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let cputype = if matches!(magic_be, 0xFEEDFACE | 0xFEEDFACF | 0xCAFEBABE) {
        u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    } else if matches!(magic_le, 0xFEEDFACE | 0xFEEDFACF | 0xBEBAFECA) {
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]])
    } else {
        return Ok(None);
    };

    match cputype {
        0x0100_0007 | 7 => Ok(Some(BinaryArch::X86_64)),
        0x0100_000C | 12 => Ok(Some(BinaryArch::Aarch64)),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;
    use zip::{ZipWriter, write::SimpleFileOptions};

    #[test]
    fn detects_elf_arch() {
        let mut bytes = vec![0_u8; 64];
        bytes[0] = 0x7f;
        bytes[1] = b'E';
        bytes[2] = b'L';
        bytes[3] = b'F';
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18] = 62;

        assert_eq!(
            detect_binary_arch(&bytes).unwrap(),
            Some(BinaryArch::X86_64)
        );
    }

    #[test]
    fn validates_web_zip_and_computes_content_hash() {
        let zip = build_web_zip("dist/__rustzen_marker__.json", br#"{"component":"web"}"#);
        let report = validate_web_dist_zip(
            &zip,
            &WebZipValidationConfig::new("dist/__rustzen_marker__.json"),
        )
        .unwrap();

        assert_eq!(report.marker_content, r#"{"component":"web"}"#);
        assert_eq!(report.content_sha256.len(), 64);
    }

    #[test]
    fn rejects_unsafe_web_zip_path() {
        let zip = build_web_zip("../config/app.env", b"bad");
        let err = validate_web_dist_zip(
            &zip,
            &WebZipValidationConfig::new("dist/__rustzen_marker__.json"),
        )
        .unwrap_err();

        assert!(err.to_string().contains("invalid paths"));
    }

    #[test]
    fn validates_deploy_signature_marker() {
        let signing_key = SigningKey::from_bytes(&[3; 32]);
        let verify_key = hex_encode(signing_key.verifying_key().as_bytes());
        let content_hash = "ab".repeat(32);
        let payload = deploy_signature_payload(
            "rustzen-admin-deploy-v1",
            "server",
            "v1.0.0",
            "x86_64",
            &content_hash,
        );
        let signature = signing_key.sign(payload.as_bytes());
        let marker = DeploySignatureMarker {
            schema_version: 1,
            component: "server".to_string(),
            version: "v1.0.0".to_string(),
            arch: "x86_64".to_string(),
            content_sha256: content_hash.clone(),
            signature: hex_encode(&signature.to_bytes()),
        };

        validate_deploy_signature_marker(
            &marker,
            &SignatureValidationInput {
                payload_version: "rustzen-admin-deploy-v1",
                expected_schema_version: 1,
                expected_component: "server",
                expected_version: "v1.0.0",
                expected_arch: "x86_64",
                expected_content_sha256: &content_hash,
                verify_key_hex: &verify_key,
            },
        )
        .unwrap();
    }

    fn build_web_zip(extra_entry: &str, extra_content: &[u8]) -> Vec<u8> {
        let mut cursor = Cursor::new(Vec::new());
        {
            let mut zip = ZipWriter::new(&mut cursor);
            let options = SimpleFileOptions::default();
            zip.start_file("dist/index.html", options).unwrap();
            zip.write_all(b"<html></html>").unwrap();
            zip.start_file("dist/assets/app.js", options).unwrap();
            zip.write_all(b"console.log('ok')").unwrap();
            zip.start_file(extra_entry, options).unwrap();
            zip.write_all(extra_content).unwrap();
            zip.finish().unwrap();
        }
        cursor.into_inner()
    }

    fn hex_encode(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
}
