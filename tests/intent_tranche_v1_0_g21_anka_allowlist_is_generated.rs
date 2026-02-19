use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    let out = h.finalize();
    hex::encode(out)
}

fn tmpdir(name: &str) -> PathBuf {
    let mut d = std::env::temp_dir();
    d.push(format!("fard_g21_{}_{}", name, std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn g21_anka_allowlist_json_matches_generator_output() {
    let src_surface = "ontology/stdlib_surface.v1_0.ontology.json";
    let committed = "spec/v1_0/anka_policy_allowed_stdlib.v1.json";
    let gen_script = "tools/gen_anka_policy_from_surface.js";

    assert!(
        PathBuf::from(src_surface).exists(),
        "g21: missing {}",
        src_surface
    );
    assert!(
        PathBuf::from(committed).exists(),
        "g21: missing {}",
        committed
    );
    assert!(
        PathBuf::from(gen_script).exists(),
        "g21: missing {}",
        gen_script
    );

    let work = tmpdir("anka_allowlist");
    let gen_path = work.join("anka_policy_allowed_stdlib.v1.json");

    let out = Command::new("node")
        .arg(gen_script)
        .arg(src_surface)
        .arg(&gen_path)
        .output()
        .unwrap();

    assert!(
        out.status.success(),
        "g21: generator failed status={:?} stdout={} stderr={}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    let a = fs::read(committed).unwrap();
    let b = fs::read(&gen_path).unwrap();

    let sha_a = sha256_hex(&a);
    let sha_b = sha256_hex(&b);

    assert!(
        a == b,
        "g21: allowlist differs from generator output\ncommitted={}\ngenerated={}\nsha_committed={}\nsha_generated={}",
        committed,
        gen_path.display(),
        sha_a,
        sha_b
    );
}
