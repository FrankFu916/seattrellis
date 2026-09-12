//! Reproducible visual QA with synthetic names only.
//! cargo run -p seattrellis-export --example render_gallery
//! Outputs are kept in a fresh private temporary directory, printed below.
use seattrellis_export::export::export_plan_with_warnings;
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // No caller-controlled destination and no existing files to overwrite.
    let output = tempfile::Builder::new()
        .prefix("seattrellis-export-gallery-")
        .tempdir()?
        .keep();
    let names = [
        "林晓雨",
        "欧阳明月",
        "Alexandra Montgomery-Johnson",
        "John Lee",
        "张伟",
        "李雨晴",
        "Amélie García",
        "李明远",
    ];
    let mut positions = Vec::new();
    let mut seats = Vec::new();
    for row in 0..7 {
        for col in 0..7 {
            if col == 3 {
                continue;
            }
            positions.push(json!([col, row]));
            seats.push(json!({"seat_id":format!("{}-{col}",char::from(b'A'+row as u8)),"row":row,"col":col,"enabled":seats.len()!=41}));
        }
    }
    let students: Vec<_> = (0..40)
        .map(|i| json!({"key":format!("S{:03}",i+1),"display_name":names[i%names.len()]}))
        .collect();
    let assignment: Vec<_> = (0..40).map(|i| json!([i, i])).collect();
    for orientation in ["landscape", "portrait"] {
        for format in ["pdf", "png", "svg", "print-html", "docx", "pptx", "xlsx"] {
            let body = json!({
                "format":format,"title":"八年级三班 · Autumn seating plan","template":"teacher",
                "locale":"zh","orientation":orientation,"paper_size":"a4","margin_mm":12,
                "show_student_ids":true,"privacy":{"hide_scores":true,"hide_notes":true,"hide_special_needs":true},
                "request":{"api_version":2,"student_count":40,"seat_positions":positions,"students":students,
                    "layout":{"layout_id":"qa","name":"QA","seats":seats}},
                "response":{"api_version":2,"feasible":true,"status":"Solved","assignment":assignment,"attempts_used":1,"hard_constraints_satisfied":true}
            });
            let (bytes, warnings) = export_plan_with_warnings(&body.to_string())?;
            let extension = if format == "print-html" {
                "html"
            } else {
                format
            };
            let path = output.join(format!("class-{orientation}.{extension}"));
            std::fs::write(&path, bytes)?;
            println!("{} {}", path.display(), warnings.join("; "));
        }
    }
    Ok(())
}
