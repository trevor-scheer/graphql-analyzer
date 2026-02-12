//! Severity types for diagnostics and lint rules.

/// Diagnostic severity level for display.
///
/// This represents the severity of a diagnostic as shown to users.
/// Maps directly to LSP's `DiagnosticSeverity`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    /// Error - indicates a problem that prevents correct execution
    Error,
    /// Warning - indicates a potential problem
    Warning,
    /// Information - informational message
    Information,
    /// Hint - a suggestion or style recommendation
    Hint,
}

impl DiagnosticSeverity {
    /// Returns true if this severity indicates an error.
    #[must_use]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }

    /// Returns true if this severity is at least a warning (warning or error).
    #[must_use]
    pub const fn is_warning_or_higher(self) -> bool {
        matches!(self, Self::Error | Self::Warning)
    }
}

impl std::fmt::Display for DiagnosticSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
            Self::Information => write!(f, "info"),
            Self::Hint => write!(f, "hint"),
        }
    }
}

/// Rule severity for lint configuration.
///
/// This represents how a lint rule should be reported, as configured
/// by the user. Rules can be turned off, reported as warnings, or as errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RuleSeverity {
    /// Rule is disabled
    Off,
    /// Rule violations are reported as warnings (default)
    #[default]
    Warn,
    /// Rule violations are reported as errors
    Error,
}

impl RuleSeverity {
    /// Returns true if the rule is enabled (warn or error).
    #[must_use]
    pub const fn is_enabled(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Convert to diagnostic severity for display.
    ///
    /// Returns `None` if the rule is off.
    #[must_use]
    pub const fn to_diagnostic_severity(self) -> Option<DiagnosticSeverity> {
        match self {
            Self::Off => None,
            Self::Warn => Some(DiagnosticSeverity::Warning),
            Self::Error => Some(DiagnosticSeverity::Error),
        }
    }
}

impl std::fmt::Display for RuleSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Warn => write!(f, "warn"),
            Self::Error => write!(f, "error"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_severity() {
        assert!(DiagnosticSeverity::Error.is_error());
        assert!(!DiagnosticSeverity::Warning.is_error());

        assert!(DiagnosticSeverity::Error.is_warning_or_higher());
        assert!(DiagnosticSeverity::Warning.is_warning_or_higher());
        assert!(!DiagnosticSeverity::Information.is_warning_or_higher());
        assert!(!DiagnosticSeverity::Hint.is_warning_or_higher());
    }

    #[test]
    fn test_diagnostic_severity_display() {
        assert_eq!(format!("{}", DiagnosticSeverity::Error), "error");
        assert_eq!(format!("{}", DiagnosticSeverity::Warning), "warning");
        assert_eq!(format!("{}", DiagnosticSeverity::Information), "info");
        assert_eq!(format!("{}", DiagnosticSeverity::Hint), "hint");
    }

    #[test]
    fn test_rule_severity() {
        assert!(!RuleSeverity::Off.is_enabled());
        assert!(RuleSeverity::Warn.is_enabled());
        assert!(RuleSeverity::Error.is_enabled());
    }

    #[test]
    fn test_rule_severity_to_diagnostic() {
        assert_eq!(RuleSeverity::Off.to_diagnostic_severity(), None);
        assert_eq!(
            RuleSeverity::Warn.to_diagnostic_severity(),
            Some(DiagnosticSeverity::Warning)
        );
        assert_eq!(
            RuleSeverity::Error.to_diagnostic_severity(),
            Some(DiagnosticSeverity::Error)
        );
    }

    #[test]
    fn test_rule_severity_default() {
        assert_eq!(RuleSeverity::default(), RuleSeverity::Warn);
    }

    #[test]
    fn test_rule_severity_display() {
        assert_eq!(format!("{}", RuleSeverity::Off), "off");
        assert_eq!(format!("{}", RuleSeverity::Warn), "warn");
        assert_eq!(format!("{}", RuleSeverity::Error), "error");
    }
}
