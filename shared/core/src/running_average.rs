use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::RwLock;

#[derive(Debug)]
struct AverageEntry {
    buffer: VecDeque<f64>,
    max_size: usize,
    sum: f64,
    all_time_pushes: usize,
}

impl AverageEntry {
    fn new(size: usize) -> Self {
        AverageEntry {
            buffer: VecDeque::with_capacity(size),
            max_size: size,
            sum: 0.0,
            all_time_pushes: 0,
        }
    }

    fn push(&mut self, value: f64) {
        if self.buffer.len() == self.max_size {
            if let Some(old_value) = self.buffer.pop_front() {
                self.sum -= old_value;
            }
        }
        self.buffer.push_back(value);
        self.sum += value;
        self.all_time_pushes += 1;
    }

    fn average(&self) -> Option<f64> {
        if self.buffer.is_empty() {
            None
        } else {
            Some(self.sum / self.buffer.len() as f64)
        }
    }
}

#[derive(Debug, Default)]
pub struct RunningAverage {
    entries: RwLock<HashMap<String, AverageEntry>>,
}

impl RunningAverage {
    pub fn new() -> Self {
        RunningAverage {
            entries: RwLock::new(HashMap::new()),
        }
    }

    pub fn add_entry_if_needed(&self, name: &str, buffer: usize) {
        let mut entries = self.entries.write().unwrap();
        if !entries.contains_key(name) {
            entries.insert(name.to_string(), AverageEntry::new(buffer));
        }
    }

    pub fn push(&self, name: &str, value: f64) {
        let mut entries = self.entries.write().unwrap();
        entries
            .get_mut(name)
            .expect("Missing RunningAverage entry")
            .push(value);
    }

    pub fn sample(&self, name: &str) -> Option<f64> {
        let entries = self.entries.read().unwrap();
        entries.get(name).and_then(|entry| entry.average())
    }

    pub fn get_all_averages(&self) -> HashMap<String, Option<f64>> {
        let entries = self.entries.read().unwrap();
        entries
            .iter()
            .map(|(name, entry)| (name.clone(), entry.average()))
            .collect()
    }

    pub fn all_time_pushes(&self, name: &str) -> Option<usize> {
        let entries = self.entries.read().unwrap();
        entries.get(name).map(|entry| entry.all_time_pushes)
    }
}
