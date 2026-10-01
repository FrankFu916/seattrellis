use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for filename in std::env::args().skip(1) {
        let started = Instant::now();
        let document = std::fs::read_to_string(&filename)?;
        let read_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let request = seattrellis_core::parse_core_solve_request(&document)?;
        let parse_seconds = started.elapsed().as_secs_f64();
        let started = Instant::now();
        let response = seattrellis_core::solve_problem(&request)?;
        let solve_seconds = started.elapsed().as_secs_f64();
        println!("{}", serde_json::json!({
            "file": filename, "read_seconds": read_seconds, "parse_seconds": parse_seconds,
            "solve_seconds": solve_seconds, "response": response,
        }));
    }
    Ok(())
}
