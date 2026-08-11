//! Fuzz target: editor command envelope parser (plan §11.4).

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let text = String::from_utf8_lossy(data);
    if let Ok(command) =
        serde_json::from_str::<seattrellis_domain::editing::EditorCommandEnvelope>(&text)
    {
        // Applying to a minimal draft must not panic either.
        let keys = ["s0", "s1", "s2"];
        let seats = (0..3)
            .map(|i| seattrellis_domain::editing::EditorSeatSpec {
                seat_id: format!("A{}", i + 1),
                row: 1,
                col: i as i32 + 1,
                enabled: true,
            })
            .collect::<Vec<_>>();
        let seat_ids: Vec<String> = (0..3).map(|i| format!("A{}", i + 1)).collect();
        let assignment: Vec<(&str, &str)> = keys
            .iter()
            .cloned()
            .zip(seat_ids.iter().map(String::as_str))
            .collect();
        if let Ok(mut draft) = seattrellis_domain::editing::EditorDraft::new(
            "fuzz",
            None,
            &keys,
            seats,
            &assignment,
            None,
        ) {
            let _ = seattrellis_domain::editing::apply_command(&mut draft, &command);
        }
    }
});
