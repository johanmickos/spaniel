pub const FC_NUMERATOR: u32 = 1;
pub const FC_DENOMINATOR: u32 = 2;

pub struct Credits {
    pub(crate) capacity: u32,
    available: u32,
}

impl Credits {
    pub fn new(capacity: u32) -> Self {
        Credits {
            capacity,
            available: capacity,
        }
    }

    /// Updates the `capacity` field, clipping the `available` credits if needed
    pub fn set_capacity(&mut self, capacity: u32) {
        self.capacity = capacity;
        if self.available > capacity {
            self.available = capacity;
        }
    }

    /// Adds available credit, up to but not exceeding `self.capacity`
    ///
    /// # Returns
    /// The updated number of available credits
    pub fn add_credit(&mut self, credit: u32) -> u32 {
        self.available += credit;
        if self.available > self.capacity {
            self.available = self.capacity;
        }
        self.available
    }

    /// Decrements available credit
    ///
    /// # Returns
    /// The updated number of available credits
    ///
    /// # Errors
    /// An error is returned if the requested number is larger than the available credit
    pub fn use_credit(&mut self, credit: u32) -> Result<u32, ()> {
        if credit > self.available {
            // Can't claim more than available credits
            return Err(());
        }
        self.available -= credit;
        Ok(self.available)
    }

    pub fn has_capacity(&self, requested: u32) -> bool {
        self.available >= requested
    }
    pub fn capacity(&self) -> u32 {
        self.capacity
    }
    pub fn available(&self) -> u32 {
        self.available
    }
}


