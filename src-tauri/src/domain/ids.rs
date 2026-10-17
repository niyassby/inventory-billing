/// New primary key: UUIDv7, so ids sort by creation time and index inserts stay append-like.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::new_id;

    #[test]
    fn ids_are_v7_and_time_ordered() {
        let a = new_id();
        // Different milliseconds, so ordering doesn't depend on the in-millisecond random bits.
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = new_id();
        assert_eq!(uuid::Uuid::parse_str(&a).unwrap().get_version_num(), 7);
        assert!(a < b, "{a} should sort before {b}");
    }
}
