use crate::error::{ForgeError, ForgeResult};
use crate::listing::ForgeListing;
use sha2::{Digest, Sha256};

pub struct Publisher;

impl Publisher {
    pub fn validate(listing: &ForgeListing) -> ForgeResult<()> {
        if listing.name.is_empty() {
            return Err(ForgeError::Validation("name is required".into()));
        }
        if listing.name.contains(' ') {
            return Err(ForgeError::Validation(
                "name must not contain spaces (use hyphens)".into(),
            ));
        }
        if listing.description.is_empty() {
            return Err(ForgeError::Validation("description is required".into()));
        }
        if listing.version.is_empty() {
            return Err(ForgeError::Validation("version is required".into()));
        }
        if semver::Version::parse(&listing.version).is_err() {
            return Err(ForgeError::Validation(format!(
                "invalid semver version: {}",
                listing.version
            )));
        }
        if listing.author.id.is_empty() || listing.author.name.is_empty() {
            return Err(ForgeError::Validation("author id and name are required".into()));
        }
        Ok(())
    }

    pub fn compute_hash(content: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content);
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::*;
    use chrono::Utc;

    fn valid_listing() -> ForgeListing {
        ForgeListing {
            id: "test".to_string(),
            name: "my-agent".to_string(),
            display_name: "My Agent".to_string(),
            description: "An agent".to_string(),
            long_description: String::new(),
            category: ForgeCategory::Agent,
            listing_type: ListingType::Free,
            author: Author {
                id: "a1".to_string(),
                name: "Dev".to_string(),
                email: None,
                url: None,
                verified: false,
            },
            version: "1.0.0".to_string(),
            license: "MIT".to_string(),
            tags: vec![],
            downloads: 0,
            rating: 0.0,
            review_count: 0,
            verified: false,
            revenue_share: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            manifest_hash: String::new(),
        }
    }

    #[test]
    fn test_valid_listing_passes() {
        assert!(Publisher::validate(&valid_listing()).is_ok());
    }

    #[test]
    fn test_empty_name_fails() {
        let mut l = valid_listing();
        l.name = String::new();
        assert!(Publisher::validate(&l).is_err());
    }

    #[test]
    fn test_spaces_in_name_fails() {
        let mut l = valid_listing();
        l.name = "my agent".to_string();
        assert!(Publisher::validate(&l).is_err());
    }

    #[test]
    fn test_bad_semver_fails() {
        let mut l = valid_listing();
        l.version = "not-a-version".to_string();
        assert!(Publisher::validate(&l).is_err());
    }

    #[test]
    fn test_hash_deterministic() {
        let h1 = Publisher::compute_hash(b"hello world");
        let h2 = Publisher::compute_hash(b"hello world");
        assert_eq!(h1, h2);
        assert_ne!(h1, Publisher::compute_hash(b"other"));
    }
}
