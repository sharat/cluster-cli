//! Shared formatting utilities for the UI

/// Format CPU millicores to a human-readable string
pub fn cpu(millicores: u64) -> String {
    if millicores == 0 {
        "-".to_string()
    } else if millicores >= 1000 {
        format_compact_unit(millicores as f64 / 1000.0, "c")
    } else {
        format!("{}m", millicores)
    }
}

/// Format memory in MB to a human-readable string
pub fn memory(mb: u64) -> String {
    if mb == 0 {
        "-".to_string()
    } else if mb >= 1024 {
        format_compact_unit(mb as f64 / 1024.0, "Gi")
    } else {
        format!("{}Mi", mb)
    }
}

/// Format a value with a suffix, showing decimals only when needed
fn format_compact_unit(value: f64, suffix: &str) -> String {
    if (value.fract() - 0.0).abs() < f64::EPSILON {
        format!("{}{suffix}", value as u64)
    } else {
        format!("{value:.1}{suffix}")
    }
}

/// Truncate a string to a maximum length without ellipsis
pub fn truncate_no_ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_formatting() {
        assert_eq!(cpu(0), "-");
        assert_eq!(cpu(500), "500m");
        assert_eq!(cpu(1000), "1c");
        assert_eq!(cpu(1500), "1.5c");
        assert_eq!(cpu(2000), "2c");
    }

    #[test]
    fn test_memory_formatting() {
        assert_eq!(memory(0), "-");
        assert_eq!(memory(512), "512Mi");
        assert_eq!(memory(1024), "1Gi");
        assert_eq!(memory(1536), "1.5Gi");
        assert_eq!(memory(2048), "2Gi");
    }

    #[test]
    fn test_truncate_no_ellipsis() {
        assert_eq!(truncate_no_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_no_ellipsis("hello world", 5), "hello");
    }
}
