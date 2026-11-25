//! Regression for original PuddleDetect pixel shaders. The original title
//! containers are extracted offline; no title execution or cache access.

#[cfg(all(windows, feature = "host"))]
#[test]
#[ignore = "requires VICTORY_DESCRIPTOR_FIXTURES with the two original .cdxil files and D3D12"]
fn indexed_image_dimension_reads_validate_and_create_pipelines() {
    use std::path::Path;
    use victory_shader::HwStage;
    use victory_shader::emit::{self, host, validator::Validator};

    let dir = std::env::var("VICTORY_DESCRIPTOR_FIXTURES")
        .expect("VICTORY_DESCRIPTOR_FIXTURES must point to the original extracted containers");
    let mut validator =
        Validator::load(Validator::default_path().as_deref()).expect("DXIL validator required");
    let host = host::Host::new(false).expect("D3D12 host required");
    for hash in ["2fd01fca16993001", "6b6db9f1a526daeb"] {
        let paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "cdxil"))
            .filter(|p| p.file_name().unwrap().to_string_lossy().contains(hash))
            .collect();
        assert_eq!(paths.len(), 1, "{hash}: exactly one original container");
        let path: &Path = &paths[0];
        let bytes = std::fs::read(path).unwrap();
        let shader = emit::translate(&bytes).unwrap_or_else(|e| panic!("{hash}: {e}"));
        assert_eq!(shader.stage, HwStage::Ps, "{hash}");
        validator
            .validate(&mut shader.dxil.clone())
            .unwrap_or_else(|e| panic!("{hash}: DXIL validation: {e}"));
        let vs = host::stub_vs(&shader).unwrap();
        host.pipeline(&vs, &shader)
            .unwrap_or_else(|e| panic!("{hash}: PSO: {e}"));
    }
}
