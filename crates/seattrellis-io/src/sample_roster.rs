//! Embedded sample roster (PD-D10-ONBOARDING, M5-A6).
//!
//! A 20-student, full-seat sample (5x4 classroom) used by the onboarding
//! flow when a teacher has no roster at hand. Design constraints from the
//! sample-roster draft: common Chinese names, balanced attributes covering
//! every common rule (vision-front, height-back, score pairing, tags,
//! notes), no rare characters, and a build-time validation gate so the
//! asset can never silently rot.
//!
//! The sample is an in-app static asset, NOT a CLI command (PD-D15 removed
//! init-demo); isolation semantics (sample namespace, one-click delete)
//! land with the class/project workflow (M5-B).

/// The sample roster as a CSV document with a header row.
pub const SAMPLE_ROSTER_CSV: &str = "\
student_id,name,gender,height_cm,score,vision,tags,needs,notes\n\
S01,张伟,M,180,95,,leader,vision_front,\n\
S02,王芳,F,172,88,,,,\n\
S03,李娜,F,165,82,,,,家长会关注\n\
S04,刘洋,M,158,76,,,,\n\
S05,陈静,F,151,70,0.6,,vision_front,\n\
S06,杨帆,M,146,64,,,,\n\
S07,赵磊,M,168,91,,,,\n\
S08,黄敏,F,138,59,,,,家长会关注\n\
S09,周涛,M,177,86,,,,\n\
S10,吴霞,F,133,55,0.6,,vision_front,\n\
S11,徐强,M,170,93,,,,\n\
S12,孙丽,F,160,79,,leader,,\n\
S13,马超,M,149,68,,,,\n\
S14,朱婷,F,142,62,,,,\n\
S15,胡军,M,162,73,0.6,,vision_front,\n\
S16,郭雪,F,176,90,,,,\n\
S17,林峰,M,155,66,,,,\n\
S18,何娟,F,148,61,,,,\n\
S19,高翔,M,135,57,,,,\n\
S20,罗丹,F,130,98,poor,,,\n\
";

/// Expected attribute coverage (mirrors the sample-roster draft).
pub const SAMPLE_ROSTER_SIZE: usize = 20;
pub const SAMPLE_ROSTER_LEADERS: usize = 2;
pub const SAMPLE_ROSTER_VISION_FRONT: usize = 4;

/// Parse the embedded sample roster into typed students (mapped columns).
/// Fails loudly if the embedded asset is malformed (build-time contract).
pub fn load_sample_roster() -> Result<Vec<crate::roster::Student>, String> {
    crate::roster::parse_roster_students(SAMPLE_ROSTER_CSV.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn sample_roster_parses_with_full_field_coverage() {
        let students = load_sample_roster().expect("sample roster must parse");
        assert_eq!(students.len(), SAMPLE_ROSTER_SIZE, "20 students");

        // No duplicate student ids.
        let ids: Vec<String> = students
            .iter()
            .filter_map(|s| s.student_id.clone())
            .collect();
        let unique: HashSet<&str> = ids.iter().map(String::as_str).collect();
        assert_eq!(ids.len(), unique.len(), "student ids must be unique");

        // Attribute coverage.
        let leaders = students
            .iter()
            .filter(|s| s.tags.iter().any(|t| t == "leader"))
            .count();
        let vision_front = students
            .iter()
            .filter(|s| s.needs.iter().any(|n| n == "vision_front"))
            .count();
        assert_eq!(leaders, SAMPLE_ROSTER_LEADERS);
        assert_eq!(vision_front, SAMPLE_ROSTER_VISION_FRONT);
        // The two noted students carry their note; blank cells parse to None.
        let noted: Vec<&str> = students
            .iter()
            .filter_map(|s| s.notes.as_deref().filter(|n| !n.is_empty()))
            .collect();
        assert_eq!(noted, vec!["家长会关注", "家长会关注"]);
    }

    #[test]
    fn sample_roster_has_no_rare_characters_and_balanced_genders() {
        let students = load_sample_roster().expect("sample roster must parse");
        let females = students
            .iter()
            .filter(|s| s.gender.as_deref() == Some("F"))
            .count();
        assert_eq!(females, SAMPLE_ROSTER_SIZE / 2, "balanced genders");
        for student in &students {
            assert!(
                student.name.as_deref().unwrap_or("").chars().all(|ch| !ch.is_ascii()),
                "names should be CJK: {:?}",
                student.name
            );
        }
    }

    #[test]
    fn sample_roster_heights_and_scores_cover_ranges() {
        let students = load_sample_roster().expect("sample roster must parse");
        let heights: Vec<f64> = students.iter().filter_map(|s| s.height_cm).collect();
        let scores: Vec<f64> = students.iter().filter_map(|s| s.score).collect();
        assert_eq!(heights.len(), SAMPLE_ROSTER_SIZE, "all heights present");
        assert_eq!(scores.len(), SAMPLE_ROSTER_SIZE, "all scores present");
        assert!(heights.iter().all(|h| (130.0..=180.0).contains(h)), "height range");
        assert!(scores.iter().all(|s| (55.0..=98.0).contains(s)), "score range");
    }
}
