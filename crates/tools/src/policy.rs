use domain::prototype::{Document, DocumentRequest, Permission};
#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Permit,
    ApprovalRequired,
    Deny(&'static str),
}

pub fn read_document(
    request: &DocumentRequest,
    envelope: &Document,
    permission: Permission,
    approval: Option<bool>,
    revoked: bool,
) -> Decision {
    if request.document != *envelope {
        return Decision::Deny("ScopeMismatch");
    }
    if revoked {
        return Decision::Deny("Revoked");
    }
    if permission == Permission::Deny {
        return Decision::Deny("ExplicitDeny");
    }
    if approval == Some(false) {
        return Decision::Deny("UserRejected");
    }
    if permission == Permission::Ask && approval != Some(true) {
        return Decision::ApprovalRequired;
    }
    Decision::Permit
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn approvals_cannot_override_scope_hard_deny_or_revocation() {
        let document = Document {
            id: "id".into(),
            name: "notes.md".into(),
            bytes: 4,
            sha256: "hash".into(),
        };
        let mut request = DocumentRequest {
            id: "request".into(),
            document: document.clone(),
        };
        assert_eq!(
            read_document(&request, &document, Permission::Ask, None, false),
            Decision::ApprovalRequired
        );
        assert_eq!(
            read_document(&request, &document, Permission::Ask, Some(true), false),
            Decision::Permit
        );
        assert_eq!(
            read_document(&request, &document, Permission::Deny, Some(true), false),
            Decision::Deny("ExplicitDeny")
        );
        assert_eq!(
            read_document(&request, &document, Permission::Allow, Some(true), true),
            Decision::Deny("Revoked")
        );
        request.document.sha256 = "changed".into();
        assert_eq!(
            read_document(&request, &document, Permission::Allow, Some(true), false),
            Decision::Deny("ScopeMismatch")
        );
    }
}
