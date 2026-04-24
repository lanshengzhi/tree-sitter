/// A small module with a few functions.
pub mod utils {
    /// Add two numbers.
    pub fn add(a: i32, b: i32) -> i32 {
        a + b
    }

    /// Subtract two numbers.
    pub fn sub(a: i32, b: i32) -> i32 {
        a - b
    }

    /// A simple struct.
    pub struct Point {
        pub x: f64,
        pub y: f64,
    }

    impl Point {
        pub fn distance(&self, other: &Point) -> f64 {
            let dx = self.x - other.x;
            let dy = self.y - other.y;
            (dx * dx + dy * dy).sqrt()
        }
    }
}
