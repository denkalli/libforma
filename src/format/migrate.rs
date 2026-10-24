//! Upgrades `document.json` from older format versions.
//!
//! Each migration takes the JSON of version `n` and returns the JSON of version `n + 1`.
//! Migrations work on raw JSON so they never depend on the current Rust types.

use serde_json::{Map, Value, json};

use crate::error::{Error, Result};

use super::FORMAT_VERSION;

type Migration = fn(Value) -> Result<Value>;

/// `MIGRATIONS[i]` upgrades version `i + 1` to version `i + 2`.
const MIGRATIONS: &[Migration] = &[v1_to_v2];

const _: () = assert!(MIGRATIONS.len() as u32 + 1 == FORMAT_VERSION);

/// Brings a document from `version` up to [`FORMAT_VERSION`].
pub(super) fn migrate(mut document: Value, version: u32) -> Result<Value> {
    if version == 0 {
        return Err(Error::InvalidFile("format version 0 does not exist".into()));
    }
    if version > FORMAT_VERSION {
        return Err(Error::UnsupportedVersion {
            found: version,
            supported: FORMAT_VERSION,
        });
    }
    for migration in &MIGRATIONS[(version - 1) as usize..] {
        document = migration(document)?;
    }
    Ok(document)
}

/// Version 2 made pages infinite. A version 1 page had a fixed `size` and a `background`;
/// it becomes a page holding one frame with that size and fill, containing the old nodes.
fn v1_to_v2(mut document: Value) -> Result<Value> {
    let invalid = |what: &str| Error::InvalidFile(format!("version 1 document: {what}"));
    let pages = document
        .get_mut("pages")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| invalid("missing pages"))?;
    for page in pages {
        let page = page
            .as_object_mut()
            .ok_or_else(|| invalid("page is not an object"))?;
        let Some(size) = page.remove("size") else {
            continue;
        };
        let background = page.remove("background");
        let children = page.remove("children").unwrap_or_else(|| json!([]));
        let name = page.get("name").cloned().unwrap_or_else(|| json!("Quadro"));

        let mut frame = Map::new();
        frame.insert("type".into(), json!("frame"));
        frame.insert("size".into(), size);
        if let Some(Value::String(color)) = background {
            frame.insert("fill".into(), json!({ "type": "solid", "color": color }));
        }
        frame.insert("children".into(), children);
        page.insert(
            "children".into(),
            json!([{
                "id": uuid::Uuid::new_v4().to_string(),
                "name": name,
                "kind": frame,
            }]),
        );
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_pages_become_frames() {
        let v1 = json!({
            "title": "t",
            "pages": [{
                "id": "dc38ae0c-45c2-44fc-bf39-55ded5d0cc0e",
                "name": "Capa",
                "size": [100.0, 50.0],
                "background": "#ff0000ff",
                "children": [{ "id": "bb41e91e-e299-4bec-a77f-a8657021086e", "kind": { "type": "group", "children": [] } }]
            }, {
                "id": "74689e51-9e9a-4b5f-b325-5a4a963ff67e",
                "size": [10.0, 10.0]
            }]
        });
        let v2 = migrate(v1, 1).unwrap();
        let page = &v2["pages"][0];
        assert!(page.get("size").is_none() && page.get("background").is_none());
        let frame = &page["children"][0];
        assert_eq!(frame["name"], "Capa");
        assert_eq!(frame["kind"]["type"], "frame");
        assert_eq!(frame["kind"]["size"], json!([100.0, 50.0]));
        assert_eq!(frame["kind"]["fill"]["color"], "#ff0000ff");
        assert_eq!(
            frame["kind"]["children"][0]["id"],
            "bb41e91e-e299-4bec-a77f-a8657021086e"
        );
        // A transparent v1 page gives a frame without fill.
        assert!(v2["pages"][1]["children"][0]["kind"].get("fill").is_none());
        let doc: crate::Document = serde_json::from_value(v2).unwrap();
        assert!(doc.pages[0].children[0].is_frame());
    }
}
