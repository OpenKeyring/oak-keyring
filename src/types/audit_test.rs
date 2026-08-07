use super::audit::AuditOperation;

#[test]
fn ssh_sign_variant_roundtrips() {
    let op = AuditOperation::SshSign;
    // The enum's db-string mapping uses dotted identifiers (e.g. "record.create",
    // "dek.rotated"); SshSign follows the same convention as "ssh.sign".
    assert_eq!(op.to_db_str(), "ssh.sign");
    assert_eq!(
        AuditOperation::from_db_str("ssh.sign").unwrap(),
        AuditOperation::SshSign
    );
}

#[test]
fn ssh_sign_rejects_unknown_db_strings() {
    // Consistent with existing behavior: unknown mapping strings yield
    // InvalidAuditOperation rather than panicking.
    assert!(AuditOperation::from_db_str("ssh_sign_typo").is_err());
}
