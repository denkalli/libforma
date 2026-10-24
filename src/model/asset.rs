use std::collections::BTreeMap;
use std::sync::Arc;

use crate::id::AssetId;

/// Binary content referenced by the document, such as an image.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Asset {
    /// IANA media type, e.g. `image/png`.
    pub media_type: String,
    pub data: Arc<[u8]>,
}

impl Asset {
    /// Guesses the media type of image data from its first bytes.
    pub fn sniff_media_type(data: &[u8]) -> Option<&'static str> {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some("image/png")
        } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
            Some("image/jpeg")
        } else {
            None
        }
    }

    /// File extension used when storing the asset.
    pub fn extension(&self) -> &str {
        match self.media_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            _ => "bin",
        }
    }
}

/// Assets of a document, keyed by content hash.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct AssetStore {
    assets: BTreeMap<AssetId, Asset>,
}

impl AssetStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds content and returns its id. Adding the same content twice stores it once.
    pub fn insert(&mut self, media_type: impl Into<String>, data: impl Into<Arc<[u8]>>) -> AssetId {
        let data = data.into();
        let id = AssetId::for_content(&data);
        self.assets.entry(id).or_insert_with(|| Asset {
            media_type: media_type.into(),
            data,
        });
        id
    }

    /// Adds image data, detecting PNG or JPEG. Returns `None` for other formats.
    pub fn insert_image(&mut self, data: impl Into<Arc<[u8]>>) -> Option<AssetId> {
        let data = data.into();
        let media_type = Asset::sniff_media_type(&data)?;
        Some(self.insert(media_type, data))
    }

    pub fn get(&self, id: AssetId) -> Option<&Asset> {
        self.assets.get(&id)
    }

    pub fn contains(&self, id: AssetId) -> bool {
        self.assets.contains_key(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (AssetId, &Asset)> {
        self.assets.iter().map(|(id, asset)| (*id, asset))
    }

    pub fn len(&self) -> usize {
        self.assets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets.is_empty()
    }

    /// Removes assets for which `keep` returns false.
    pub fn retain(&mut self, mut keep: impl FnMut(AssetId) -> bool) {
        self.assets.retain(|id, _| keep(*id));
    }
}
