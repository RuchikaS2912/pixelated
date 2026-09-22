//! Sequential animation queue (requirement 14): when several reminders
//! fire at once they are displayed one after another instead of
//! overlapping. `max_simultaneous` (default 1) controls concurrency.

#[derive(Debug, Clone)]
pub struct PendingShow {
    pub message: String,
    pub character: Option<String>,
    /// Debug label used in logs (reminder title).
    pub label: String,
}

#[derive(Debug, Default)]
pub struct ShowQueue {
    items: std::collections::VecDeque<PendingShow>,
    max: usize,
}

impl ShowQueue {
    pub fn new(max_simultaneous: usize) -> ShowQueue {
        ShowQueue {
            items: Default::default(),
            max: max_simultaneous.clamp(1, 4),
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn push(&mut self, item: PendingShow) {
        // Bound the queue so a runaway schedule can't pile up forever;
        // oldest non-displayed entries are dropped (missed-after-wake
        // coalescing already ensures one pending entry per reminder).
        if self.items.len() >= 32 {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    /// Take up to `free_slots` items for immediate display.
    pub fn take(&mut self, free_slots: usize) -> Vec<PendingShow> {
        let n = free_slots.min(self.items.len());
        self.items.drain(..n).collect()
    }

    pub fn capacity(&self) -> usize {
        self.max
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(i: u32) -> PendingShow {
        PendingShow {
            message: format!("msg {i}"),
            character: None,
            label: format!("r{i}"),
        }
    }

    #[test]
    fn sequential_when_max_one() {
        let mut q = ShowQueue::new(1);
        for i in 0..3 {
            q.push(item(i));
        }
        assert_eq!(q.len(), 3);
        let batch = q.take(1);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].label, "r0"); // FIFO order
        let batch2 = q.take(1);
        assert_eq!(batch2[0].label, "r1");
    }

    #[test]
    fn respects_slots() {
        let mut q = ShowQueue::new(2);
        for i in 0..5 {
            q.push(item(i));
        }
        assert_eq!(q.take(2).len(), 2);
        assert_eq!(q.take(0).len(), 0);
        assert_eq!(q.take(9).len(), 3);
        assert!(q.is_empty());
    }

    #[test]
    fn bounded() {
        let mut q = ShowQueue::new(1);
        for i in 0..100 {
            q.push(item(i));
        }
        assert!(q.len() <= 32);
    }
}
