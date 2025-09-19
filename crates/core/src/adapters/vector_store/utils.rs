pub fn build_index_key<'a, I>(parts: I) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    let mut first = true;
    for part in parts {
        if !first {
            hasher.update([0x1f]);
        }
        hasher.update(part.as_bytes());
        first = false;
    }
    let digest = hasher.finalize();
    hex::encode(digest)
}
