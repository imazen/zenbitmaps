//! zennode node definitions for BMP encoding.
//!
//! Defines [`EncodeBmp`] with RIAPI-compatible querystring keys for
//! BMP bit depth selection.

extern crate alloc;

use zennode::*;

/// BMP encoding with bit depth selection.
///
/// Supports 24-bit RGB (no alpha) and 32-bit RGBA (with alpha channel).
/// BMP is uncompressed, so output size is determined entirely by
/// dimensions and bit depth.
///
/// JSON API: `{ "bits": 24 }`
/// RIAPI: `?bmp.bits=32`
#[derive(Node, Clone, Debug)]
#[node(id = "zenbitmaps.encode_bmp", group = Encode, role = Encode)]
#[node(tags("codec", "bmp", "lossless", "encode"))]
pub struct EncodeBmp {
    /// Bit depth: 24 (RGB, no alpha) or 32 (RGBA with alpha).
    ///
    /// 24-bit BMP stores pixels as BGR with no alpha channel.
    /// 32-bit BMP stores pixels as BGRA, preserving transparency.
    /// Most applications expect 24-bit BMP.
    #[param(range(1..=32), default = 24, step = 8)]
    #[param(unit = "bits", section = "Main", label = "Bit Depth")]
    #[kv("bmp.bits", "bits")]
    pub bits: i32,
}

impl Default for EncodeBmp {
    fn default() -> Self {
        Self { bits: 24 }
    }
}

/// Registration function for aggregating crates.
pub fn register(registry: &mut NodeRegistry) {
    registry.register(&ENCODE_BMP_NODE);
}

/// All BMP zennode definitions.
pub static ALL: &[&dyn NodeDef] = &[&ENCODE_BMP_NODE];

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn schema_metadata() {
        let schema = ENCODE_BMP_NODE.schema();
        assert_eq!(schema.id, "zenbitmaps.encode_bmp");
        assert_eq!(schema.group, NodeGroup::Encode);
        assert_eq!(schema.role, NodeRole::Encode);
        assert!(schema.tags.contains(&"codec"));
        assert!(schema.tags.contains(&"bmp"));
        assert!(schema.tags.contains(&"lossless"));
        assert!(schema.tags.contains(&"encode"));
    }

    #[test]
    fn param_count_and_names() {
        let schema = ENCODE_BMP_NODE.schema();
        let names: Vec<&str> = schema.params.iter().map(|p| p.name).collect();
        assert_eq!(names.len(), 1);
        assert!(names.contains(&"bits"));
    }

    #[test]
    fn defaults() {
        let node = ENCODE_BMP_NODE.create_default().unwrap();
        assert_eq!(node.get_param("bits"), Some(ParamValue::I32(24)));
    }

    #[test]
    fn set_32bit() {
        let mut params = ParamMap::new();
        params.insert("bits".into(), ParamValue::I32(32));
        let node = ENCODE_BMP_NODE.create(&params).unwrap();
        assert_eq!(node.get_param("bits"), Some(ParamValue::I32(32)));
    }

    #[test]
    fn from_kv_bits() {
        let mut kv = KvPairs::from_querystring("bmp.bits=32");
        let node = ENCODE_BMP_NODE.from_kv(&mut kv).unwrap().unwrap();
        assert_eq!(node.get_param("bits"), Some(ParamValue::I32(32)));
        assert_eq!(kv.unconsumed().count(), 0);
    }

    #[test]
    fn from_kv_alias() {
        let mut kv = KvPairs::from_querystring("bits=32");
        let node = ENCODE_BMP_NODE.from_kv(&mut kv).unwrap().unwrap();
        assert_eq!(node.get_param("bits"), Some(ParamValue::I32(32)));
    }

    #[test]
    fn from_kv_no_match() {
        let mut kv = KvPairs::from_querystring("w=800&h=600");
        let result = ENCODE_BMP_NODE.from_kv(&mut kv).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn json_round_trip() {
        let mut params = ParamMap::new();
        params.insert("bits".into(), ParamValue::I32(32));

        let node = ENCODE_BMP_NODE.create(&params).unwrap();
        assert_eq!(node.get_param("bits"), Some(ParamValue::I32(32)));

        // Round-trip
        let exported = node.to_params();
        let node2 = ENCODE_BMP_NODE.create(&exported).unwrap();
        assert_eq!(node2.get_param("bits"), Some(ParamValue::I32(32)));
    }

    #[test]
    fn downcast_to_concrete() {
        let node = ENCODE_BMP_NODE.create_default().unwrap();
        let enc = node.as_any().downcast_ref::<EncodeBmp>().unwrap();
        assert_eq!(enc.bits, 24);
    }

    #[test]
    fn registry_integration() {
        let mut registry = NodeRegistry::new();
        register(&mut registry);
        assert!(registry.get("zenbitmaps.encode_bmp").is_some());

        let result = registry.from_querystring("bmp.bits=32");
        assert_eq!(result.instances.len(), 1);
        assert_eq!(result.instances[0].schema().id, "zenbitmaps.encode_bmp");
    }
}
