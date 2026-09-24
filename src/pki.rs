use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, ExtendedKeyUsagePurpose,
    IsCa, Issuer, KeyPair, KeyUsagePurpose,
};
use std::path::Path;

pub fn certificate_expires(pem: &[u8]) -> Result<i64> {
    let (_, pem) =
        x509_parser::pem::parse_x509_pem(pem).map_err(|_| anyhow::anyhow!("证书 PEM 格式无效"))?;
    let cert = pem
        .parse_x509()
        .map_err(|_| anyhow::anyhow!("证书 DER 格式无效"))?;
    Ok(cert.validity().not_after.timestamp())
}

pub fn private_write(path: &Path, bytes: impl AsRef<[u8]>) -> Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut f = options.open(path)?;
    f.write_all(bytes.as_ref())?;
    f.sync_all()?;
    Ok(())
}
pub fn initialize(dir: &Path, names: Vec<String>) -> Result<()> {
    anyhow::ensure!(!dir.join("ca-key.pem").exists(), "CA 已存在，拒绝覆盖");
    let key = KeyPair::generate()?;
    let mut ca = CertificateParams::new(Vec::<String>::new())?;
    ca.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
    ca.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    ca.distinguished_name
        .push(rcgen::DnType::CommonName, "opsd 控制机构");
    let cert = ca.self_signed(&key)?;
    let issuer = Issuer::new(ca, key);
    private_write(&dir.join("ca.pem"), cert.pem())?;
    private_write(&dir.join("ca-key.pem"), issuer.key().serialize_pem())?;
    let server_key = KeyPair::generate()?;
    let mut params = CertificateParams::new(names)?;
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let server = params.signed_by(&server_key, &issuer)?;
    private_write(&dir.join("server.pem"), server.pem())?;
    private_write(&dir.join("server-key.pem"), server_key.serialize_pem())?;
    Ok(())
}
pub fn sign_csr(dir: &Path, csr: &str, node_id: &str) -> Result<String> {
    let key = KeyPair::from_pem(&std::fs::read_to_string(dir.join("ca-key.pem"))?)?;
    let issuer = Issuer::from_ca_cert_pem(&std::fs::read_to_string(dir.join("ca.pem"))?, key)?;
    let mut request = CertificateSigningRequestParams::from_pem(csr)?;
    request.params.is_ca = IsCa::NoCa;
    request.params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    request.params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    request.params.subject_alt_names = vec![];
    request.params.distinguished_name = rcgen::DistinguishedName::new();
    request
        .params
        .distinguished_name
        .push(rcgen::DnType::CommonName, node_id);
    request.params.not_before = time::OffsetDateTime::now_utc() - time::Duration::minutes(5);
    request.params.not_after = time::OffsetDateTime::now_utc() + time::Duration::days(90);
    Ok(request.signed_by(&issuer)?.pem())
}
pub fn certs(path: &Path) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    rustls_pemfile::certs(&mut std::io::BufReader::new(std::fs::File::open(path)?))
        .collect::<std::io::Result<Vec<_>>>()
        .map_err(Into::into)
}
pub fn key(path: &Path) -> Result<rustls::pki_types::PrivateKeyDer<'static>> {
    rustls_pemfile::private_key(&mut std::io::BufReader::new(std::fs::File::open(path)?))?
        .context("缺少私钥")
}
pub fn cert_fingerprint(pem: &str) -> Result<String> {
    let cert = rustls_pemfile::certs(&mut std::io::Cursor::new(pem.as_bytes()))
        .next()
        .context("缺少证书")??;
    Ok(crate::protocol::digest(cert.as_ref()))
}
pub fn client_config(dir: &Path) -> Result<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    for c in certs(&dir.join("ca.pem"))? {
        roots.add(c)?;
    }
    Ok(rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(
            certs(&dir.join("client.pem"))?,
            key(&dir.join("client-key.pem"))?,
        )?)
}
