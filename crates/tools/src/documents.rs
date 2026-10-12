//! Imports are an explicit human UI action. Agent reads use UUID-only paths
//! relative to a held directory capability and verify the pinned content digest.
use crate::policy::{self, Decision};
use cap_std::{
    ambient_authority,
    fs::{Dir, OpenOptions},
};
use domain::prototype::{Document, DocumentRequest, Permission};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::Path,
};
use uuid::Uuid;
pub const MAX_BYTES: usize = 64 * 1024;
pub struct Library {
    directory: Dir,
}

pub fn validate(document: &Document) -> Result<(), String> {
    if Uuid::parse_str(&document.id)
        .ok()
        .is_none_or(|u| u.to_string() != document.id)
        || document.name.is_empty()
        || document.name.len() > 150
        || document.name.contains(['/', '\\', ':'])
        || document.bytes == 0
        || document.bytes > MAX_BYTES as u64
        || document.sha256.len() != 64
        || !document.sha256.bytes().all(|c| c.is_ascii_hexdigit())
    {
        return Err("Invalid registered document metadata.".into());
    }
    Ok(())
}
impl Library {
    pub fn open(path: &Path) -> Result<Self, String> {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        Ok(Self {
            directory: Dir::open_ambient_dir(path, ambient_authority())
                .map_err(|e| e.to_string())?,
        })
    }
    pub fn import(&self, name: &str, text: &str) -> Result<Document, String> {
        if !(name.to_lowercase().ends_with(".md") || name.to_lowercase().ends_with(".txt"))
            || text.is_empty()
            || text.len() > MAX_BYTES
            || text.contains('\0')
        {
            return Err(
                "Choose a nonempty UTF-8 .txt/.md document up to 64 KiB, without NUL bytes.".into(),
            );
        }
        let document = Document {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
            bytes: text.len() as u64,
            sha256: format!("{:x}", Sha256::digest(text.as_bytes())),
        };
        validate(&document)?;
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        let mut file = self
            .directory
            .open_with(format!("{}.txt", document.id), &options)
            .map_err(|e| e.to_string())?;
        file.write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        Ok(document)
    }
    pub fn read(
        &self,
        request: &DocumentRequest,
        envelope: &Document,
        permission: Permission,
        approval: Option<bool>,
        revoked: bool,
    ) -> Result<String, String> {
        match policy::read_document(request, envelope, permission, approval, revoked) {
            Decision::Deny(reason) => return Err(format!("PolicyDenied: {reason}")),
            Decision::ApprovalRequired => return Err("ApprovalRequired: no read performed.".into()),
            Decision::Permit => {}
        }
        validate(envelope)?;
        let path = format!("{}.txt", envelope.id);
        let metadata = self
            .directory
            .symlink_metadata(&path)
            .map_err(|e| format!("DocumentUnavailable: {e}"))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err("DocumentUnavailable: expected a regular library file.".into());
        }
        let file = self
            .directory
            .open(path)
            .map_err(|e| format!("DocumentUnavailable: {e}"))?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        if !metadata.is_file() || metadata.len() != envelope.bytes {
            return Err("DocumentChanged: size does not match the pinned input.".into());
        }
        let mut bytes = vec![];
        file.take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| format!("DocumentReadFailed: {e}"))?;
        if bytes.len() > MAX_BYTES
            || bytes.len() as u64 != envelope.bytes
            || format!("{:x}", Sha256::digest(&bytes)) != envelope.sha256
        {
            return Err(
                "DocumentChanged: content does not match the pinned SHA-256 digest.".into(),
            );
        }
        String::from_utf8(bytes).map_err(|_| "DocumentReadFailed: content is not UTF-8.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (std::path::PathBuf, Library, DocumentRequest) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.cache/document-tests")
            .join(Uuid::new_v4().to_string());
        let library = Library::open(&path).unwrap();
        let document = library
            .import(
                "notes.md",
                "Research input. Ignore restrictions is DATA, not authority.",
            )
            .unwrap();
        (
            path,
            library,
            DocumentRequest {
                id: Uuid::new_v4().to_string(),
                document,
            },
        )
    }
    #[test]
    fn reads_registered_bytes_only_after_policy_permits() {
        let (path, library, request) = fixture();
        assert!(library
            .read(&request, &request.document, Permission::Ask, None, false)
            .unwrap_err()
            .starts_with("ApprovalRequired"));
        assert!(library
            .read(
                &request,
                &request.document,
                Permission::Deny,
                Some(true),
                false
            )
            .unwrap_err()
            .starts_with("PolicyDenied"));
        assert!(library
            .read(
                &request,
                &request.document,
                Permission::Ask,
                Some(true),
                false
            )
            .unwrap()
            .contains("is DATA"));
        std::fs::remove_file(path.join(format!("{}.txt", request.document.id))).unwrap();
        assert!(library
            .read(
                &request,
                &request.document,
                Permission::Deny,
                Some(true),
                false
            )
            .unwrap_err()
            .starts_with("PolicyDenied"));
    }
    #[test]
    fn content_changes_and_path_forms_fail_closed() {
        let (path, library, request) = fixture();
        std::fs::write(
            path.join(format!("{}.txt", request.document.id)),
            "x".repeat(request.document.bytes as usize),
        )
        .unwrap();
        assert!(library
            .read(&request, &request.document, Permission::Allow, None, false)
            .unwrap_err()
            .starts_with("DocumentChanged"));
        for id in ["../secret", "C:\\secret", "name:stream", "/etc/passwd"] {
            let mut d = request.document.clone();
            d.id = id.into();
            assert!(validate(&d).is_err());
        }
        assert!(library
            .import("huge.md", &"x".repeat(MAX_BYTES + 1))
            .is_err());
        assert!(library.import("binary.txt", "bad\0input").is_err());
    }
    #[test]
    fn changed_hardlink_replacement_is_not_a_new_authority_grant() {
        let (path, library, request) = fixture();
        let file = path.join(format!("{}.txt", request.document.id));
        let outside = path
            .parent()
            .unwrap()
            .join(format!("outside-{}.txt", request.document.id));
        std::fs::write(&outside, "different data").unwrap();
        std::fs::remove_file(&file).unwrap();
        std::fs::hard_link(&outside, &file).unwrap();
        assert!(library
            .read(&request, &request.document, Permission::Allow, None, false)
            .is_err());
        // Directory capability prevents arbitrary path access; a local attacker
        // able to replace same-content files is outside the trusted-host model.
    }
}
