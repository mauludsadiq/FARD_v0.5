use std::env;
use std::fs;

#[path = "../verify/trace_verify.rs"]
mod trace_verify;

#[path = "../verify/artifact_verify.rs"]
mod artifact_verify;

#[path = "../verify/bundle_verify.rs"]
mod bundle_verify;

fn usage() -> ! {
  eprintln!("usage:");
  eprintln!("  fardverify trace --out <dir>");
  eprintln!("  fardverify artifact --out <dir>");
  eprintln!("  fardverify bundle --out <dir>");
  std::process::exit(2);
}

fn get_out(args: &[String]) -> String {
  let mut out: Option<String> = None;
  let mut i = 0usize;
  while i < args.len() {
    if args[i] == "--out" {
      if i + 1 >= args.len() { usage(); }
      out = Some(args[i+1].clone());
      i += 2;
      continue;
    }
    i += 1;
  }
  out.unwrap_or_else(|| usage())
}

fn main() {
  let args: Vec<String> = env::args().collect();
  if args.len() < 3 { usage(); }
  let sub = &args[1];
  let outdir = get_out(&args[2..]);

  if sub == "trace" {
    match trace_verify::verify_trace_outdir(&outdir) {
      Ok(()) => {
        let p = format!("{}/PASS_TRACE.txt", outdir);
        let _ = fs::write(&p, b"PASS\n");
        std::process::exit(0);
      }
      Err(e) => {
        let p = format!("{}/FAIL_TRACE.txt", outdir);
        let _ = fs::write(&p, format!("FAIL {}\n", e).as_bytes());
        eprintln!("TRACE_VERIFY_FAIL {}", e);
        std::process::exit(2);
      }
    }
  }

  if sub == "artifact" {
    match trace_verify::verify_trace_outdir(&outdir) {
      Ok(()) => {
        match artifact_verify::verify_artifact_outdir(&outdir) {
          Ok(()) => {
            let p = format!("{}/PASS_ARTIFACT.txt", outdir);
            let _ = fs::write(&p, b"PASS\n");
            std::process::exit(0);
          }
          Err(e) => {
            let p = format!("{}/FAIL_ARTIFACT.txt", outdir);
            let _ = fs::write(&p, format!("FAIL {}\n", e).as_bytes());
            eprintln!("ARTIFACT_VERIFY_FAIL {}", e);
            std::process::exit(2);
          }
        }
      }
      Err(e) => {
        let p = format!("{}/FAIL_ARTIFACT.txt", outdir);
        let _ = fs::write(&p, format!("FAIL {}\n", e).as_bytes());
        eprintln!("TRACE_VERIFY_FAIL {}", e);
        std::process::exit(2);
      }
    }
  }

  if sub == "bundle" {
    match bundle_verify::verify_bundle_outdir(&outdir) {
      Ok(()) => {
        let p = format!("{}/PASS_BUNDLE.txt", outdir);
        let _ = fs::write(&p, b"PASS\n");
        std::process::exit(0);
      }
      Err(e) => {
        let p = format!("{}/FAIL_BUNDLE.txt", outdir);
        let _ = fs::write(&p, format!("FAIL {}\n", e).as_bytes());
        eprintln!("BUNDLE_VERIFY_FAIL {}", e);
        std::process::exit(2);
      }
    }
  }

  usage();
}
