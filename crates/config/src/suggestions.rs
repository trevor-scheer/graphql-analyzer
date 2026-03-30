use strsim::jaro_winkler;

/// Minimum Jaro-Winkler similarity to consider a candidate a plausible match.
const SIMILARITY_THRESHOLD: f64 = 0.8;

/// Find the closest match from `candidates` for the given `input`.
///
/// Returns `None` if no candidate meets the similarity threshold.
pub fn did_you_mean<'a>(
    input: &str,
    candidates: impl IntoIterator<Item = &'a str>,
) -> Option<&'a str> {
    candidates
        .into_iter()
        .map(|c| (c, jaro_winkler(input, c)))
        .filter(|(_, score)| *score >= SIMILARITY_THRESHOLD)
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_prefix_match() {
        let candidates = [
            "noAnonymousOperations",
            "noDeprecatedUsage",
            "requireDescription",
        ];
        assert_eq!(
            did_you_mean("noAnonymous", candidates),
            Some("noAnonymousOperations")
        );
    }

    #[test]
    fn typo_match() {
        let candidates = ["recommended", "strict", "all"];
        assert_eq!(did_you_mean("recomended", candidates), Some("recommended"));
    }

    #[test]
    fn no_match_for_unrelated_input() {
        let candidates = ["recommended", "strict", "all"];
        assert_eq!(did_you_mean("completely_wrong", candidates), None);
    }

    #[test]
    fn exact_match() {
        let candidates = ["recommended", "strict"];
        assert_eq!(did_you_mean("recommended", candidates), Some("recommended"));
    }

    #[test]
    fn empty_candidates() {
        let candidates: [&str; 0] = [];
        assert_eq!(did_you_mean("anything", candidates), None);
    }

    #[test]
    fn case_sensitivity() {
        // Jaro-Winkler is case-sensitive; "Recommended" vs "recommended" should
        // still be close enough to suggest.
        let candidates = ["recommended"];
        assert_eq!(did_you_mean("Recommended", candidates), Some("recommended"));
    }

    #[test]
    fn picks_best_among_multiple_close_candidates() {
        let candidates = ["noUnusedFragments", "noUnusedVariables"];
        assert_eq!(
            did_you_mean("noUnusedFragment", candidates),
            Some("noUnusedFragments")
        );
    }
}
