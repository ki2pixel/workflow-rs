use std::env;
use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use workflow_tpu::Session;

struct Args {
    tflite: PathBuf,
    edgetpu: PathBuf,
    model: PathBuf,
    input: Option<PathBuf>,
    output: Option<PathBuf>,
    compare: Option<PathBuf>,
    iterations: usize,
    warmup: usize,
    threads: i32,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;

    let mut session = Session::open(&args.model, &args.tflite, &args.edgetpu, args.threads)?;

    println!("model={}", args.model.display());
    println!(
        "edgetpu_version={}",
        session
            .edgetpu_version()
            .unwrap_or_else(|| "inconnue".to_string())
    );
    println!("devices={:?}", session.device_paths());
    print_specs("input", session.input_specs());
    print_specs("output", session.output_specs());

    let input_bytes = match &args.input {
        Some(path) => {
            let data = fs::read(path)?;
            let expected = session.input_specs()[0].bytes;
            if data.len() != expected {
                return Err(format!(
                    "{} contient {} octets, le tenseur en attend {expected}",
                    path.display(),
                    data.len()
                )
                .into());
            }
            data
        }
        None => vec![0u8; session.input_specs()[0].bytes],
    };

    for _ in 0..args.warmup {
        session.invoke(&[input_bytes.as_slice()])?;
    }

    let iterations = args.iterations.max(1);
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        session.invoke(&[input_bytes.as_slice()])?;
        samples.push(start.elapsed().as_nanos() as u64);
    }

    samples.sort_unstable();
    let mean = samples.iter().sum::<u64>() as f64 / samples.len() as f64;
    let p50 = samples[samples.len() / 2];
    let p99 = samples[(samples.len() * 99 / 100).min(samples.len() - 1)];
    println!(
        "iterations={iterations} mean_us={:.2} p50_us={:.2} p99_us={:.2}",
        mean / 1_000.0,
        p50 as f64 / 1_000.0,
        p99 as f64 / 1_000.0
    );

    let output = session.output().to_vec();
    println!("output_bytes={}", output.len());
    println!("checksum_fnv1a64={:016x}", fnv1a(&output));
    for (index, (spec, bytes)) in session.output_segments().iter().enumerate() {
        println!(
            "segment[{index}] name={:?} bytes={} checksum={:016x}",
            spec.name,
            bytes.len(),
            fnv1a(bytes)
        );
    }

    if let Some(path) = &args.output {
        fs::write(path, &output)?;
        println!("written={}", path.display());
    }

    if let Some(path) = &args.compare {
        let reference = fs::read(path)?;
        let length_equal = reference.len() == output.len();
        let diffs = reference
            .iter()
            .zip(output.iter())
            .filter(|(left, right)| left != right)
            .count();
        let first_diff = reference
            .iter()
            .zip(output.iter())
            .position(|(left, right)| left != right);
        println!(
            "compare length_equal={length_equal} reference_bytes={} diffs={diffs} first_diff={first_diff:?}",
            reference.len()
        );
    }

    Ok(())
}

fn print_specs(prefix: &str, specs: &[workflow_tpu::TensorSpec]) {
    for (index, spec) in specs.iter().enumerate() {
        println!(
            "{prefix}[{index}] name={:?} dims={:?} type={} bytes={} scale={} zero_point={}",
            spec.name, spec.dims, spec.ty, spec.bytes, spec.scale, spec.zero_point
        );
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn parse_args() -> Result<Args, Box<dyn Error>> {
    let mut args = Args {
        tflite: PathBuf::from("vendor/libtensorflowlite_c.so"),
        edgetpu: env::var_os("WORKFLOW_EDGETPU_LIB")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/usr/lib/x86_64-linux-gnu/libedgetpu.so.1")),
        model: PathBuf::new(),
        input: None,
        output: None,
        compare: None,
        iterations: 100,
        warmup: 5,
        threads: 1,
    };

    let mut iter = env::args().skip(1);
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--tflite" => args.tflite = PathBuf::from(next_value(&mut iter, &flag)?),
            "--edgetpu" => args.edgetpu = PathBuf::from(next_value(&mut iter, &flag)?),
            "--model" => args.model = PathBuf::from(next_value(&mut iter, &flag)?),
            "--input" => args.input = Some(PathBuf::from(next_value(&mut iter, &flag)?)),
            "--output" => args.output = Some(PathBuf::from(next_value(&mut iter, &flag)?)),
            "--compare" => args.compare = Some(PathBuf::from(next_value(&mut iter, &flag)?)),
            "--iterations" => args.iterations = parse_usize(&mut iter, &flag)?,
            "--warmup" => args.warmup = parse_usize(&mut iter, &flag)?,
            "--threads" => args.threads = parse_i32(&mut iter, &flag)?,
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            other => return Err(format!("argument inconnu: {other}").into()),
        }
    }

    if args.model.as_os_str().is_empty() {
        return Err("--model est requis (utiliser --help)".into());
    }

    Ok(args)
}

fn next_value(
    iter: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<String, Box<dyn Error>> {
    iter.next()
        .ok_or_else(|| format!("valeur manquante pour {flag}").into())
}

fn parse_usize(
    iter: &mut impl Iterator<Item = String>,
    flag: &str,
) -> Result<usize, Box<dyn Error>> {
    let value = next_value(iter, flag)?;
    value
        .parse()
        .map_err(|error| format!("valeur invalide pour {flag}: {error}").into())
}

fn parse_i32(iter: &mut impl Iterator<Item = String>, flag: &str) -> Result<i32, Box<dyn Error>> {
    let value = next_value(iter, flag)?;
    value
        .parse()
        .map_err(|error| format!("valeur invalide pour {flag}: {error}").into())
}

fn print_usage() {
    println!(
        "workflow-tpu-spike --model <modele.tflite> [options]\n\
         \n\
         --tflite <libtensorflowlite_c.so>  defaut: vendor/libtensorflowlite_c.so\n\
         --edgetpu <libedgetpu.so.1>        defaut: WORKFLOW_EDGETPU_LIB ou /usr/lib/x86_64-linux-gnu/libedgetpu.so.1\n\
         --model <modele.tflite>            requis\n\
         --input <fichier brut>             defaut: zeros\n\
         --output <fichier brut>            ecrit les sorties concatenees\n\
         --compare <fichier brut>           compare avec une reference\n\
         --iterations <n>                   defaut: 100\n\
         --warmup <n>                       defaut: 5\n\
         --threads <n>                      defaut: 1"
    );
}
