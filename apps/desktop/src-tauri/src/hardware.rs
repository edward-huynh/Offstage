//! What this computer has, as far as choosing a model needs it.

/// The memory of this computer in bytes, or 0 if it cannot be read. Read once: it does not change.
pub fn total_ram() -> u64 {
    static RAM: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *RAM.get_or_init(|| {
        let mut system = sysinfo::System::new();
        system.refresh_memory();
        system.total_memory()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_memory_of_this_machine_can_be_read() {
        assert!(total_ram() > 512 << 20, "{}", total_ram());
    }
}
