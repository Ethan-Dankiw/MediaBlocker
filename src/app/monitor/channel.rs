#[derive(Clone)]
pub struct AppChannel<T> {
    /// The producer channel (sender)
    producer: async_std::channel::Sender<T>,

    /// The consumer channel (receiver)
    consumer: async_std::channel::Receiver<T>,
}

impl<T> AppChannel<T> {
    pub fn new() -> Self {
        // Create an unbounded channel
        let (producer, consumer) = async_std::channel::unbounded::<T>();
        Self { producer, consumer }
    }

    pub fn get_producer(&self) -> async_std::channel::Sender<T> {
        self.producer.clone()
    }

    pub fn get_consumer(&self) -> async_std::channel::Receiver<T> {
        self.consumer.clone()
    }
}
