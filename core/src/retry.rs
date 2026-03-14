// Retry logic and error handling

use crate::error::{GeminiAudioError, Result};
use crate::config::RetryConfig;
use std::time::Duration;
use tokio::time::sleep;

/// Retry manager for handling API errors
pub struct RetryManager {
    config: RetryConfig,
    current_retries: usize,
}

impl RetryManager {
    /// Create a new retry manager
    pub fn new(config: RetryConfig) -> Self {
        Self {
            config,
            current_retries: 0,
        }
    }

    /// Determine if we should retry based on error type
    pub fn should_retry(&self, error: &GeminiAudioError) -> bool {
        match error {
            GeminiAudioError::API(msg) => {
                // Parse HTTP status code from error message
                if msg.contains("429") {
                    self.config.retry_429
                } else if msg.contains("401") {
                    self.config.retry_401
                } else if msg.contains("5") && msg.len() >= 3 {
                    // Check for 5xx errors
                    let code_str = &msg[..3];
                    if let Ok(code) = code_str.parse::<u16>() {
                        if code >= 500 && code < 600 {
                            self.current_retries < self.config.max_retries_5xx
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
            GeminiAudioError::Network(_) => {
                self.current_retries < self.config.max_retries_5xx
            }
            GeminiAudioError::Timeout(_) => {
                // Immediate retry for timeout errors in real-time context
                true
            }
            _ => false,
        }
    }

    /// Get retry delay based on error type
    pub fn get_retry_delay(&self, error: &GeminiAudioError) -> Duration {
        match error {
            GeminiAudioError::API(msg) => {
                if msg.contains("429") {
                    // Check for Retry-After in message: e.g., "(Retry-After: 30)"
                    if let Some(start) = msg.find("(Retry-After: ") {
                        let after_start = &msg[start + 14..];
                        if let Some(end) = after_start.find(')') {
                            if let Ok(secs) = after_start[..end].parse::<u64>() {
                                return Duration::from_secs(secs);
                            }
                        }
                    }
                    Duration::from_millis(self.config.retry_delay_ms)
                } else if msg.contains("timeout") {
                    Duration::from_millis(self.config.retry_delay_ms)
                } else if msg.contains("5") && msg.len() >= 3 {
                    // Exponential backoff for 5xx errors
                    let delay_ms = (self.config.retry_delay_ms as f64 
                        * self.config.backoff_factor.powi(self.current_retries as i32))
                        .min(self.config.max_backoff_ms as f64);
                    Duration::from_millis(delay_ms as u64)
                } else {
                    Duration::from_millis(0)
                }
            }
            GeminiAudioError::Network(_) => {
                // Exponential backoff for network errors
                let delay_ms = (self.config.retry_delay_ms as f64 
                    * self.config.backoff_factor.powi(self.current_retries as i32))
                    .min(self.config.max_backoff_ms as f64);
                Duration::from_millis(delay_ms as u64)
            }
            GeminiAudioError::Timeout(_) => {
                // Immediate retry for timeout errors
                Duration::from_millis(0)
            }
            _ => Duration::from_millis(0),
        }
    }

    /// Increment retry counter
    pub fn increment_retry(&mut self) {
        self.current_retries += 1;
    }

    /// Reset retry counter after successful interaction
    pub fn reset_retries(&mut self) {
        self.current_retries = 0;
    }

    /// Get current retry count
    pub fn get_retry_count(&self) -> usize {
        self.current_retries
    }

    /// Execute operation with retry logic
    pub async fn execute_with_retry<F, Fut, T>(&mut self, mut operation: F) -> Result<T>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        loop {
            match operation().await {
                Ok(result) => {
                    self.reset_retries();
                    return Ok(result);
                }
                Err(error) => {
                    if !self.should_retry(&error) {
                        return Err(error);
                    }

                    let delay = self.get_retry_delay(&error);
                    if delay.as_millis() > 0 {
                        sleep(delay).await;
                    }

                    self.increment_retry();
                    
                    if self.current_retries >= self.config.max_retries_5xx {
                        return Err(error);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_manager_creation() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);
        assert_eq!(manager.get_retry_count(), 0);
    }

    #[test]
    fn test_should_retry_429() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);
        
        let error = GeminiAudioError::API("429 Too Many Requests".to_string());
        assert!(manager.should_retry(&error));
    }

    #[test]
    fn test_should_retry_401() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);
        
        let error = GeminiAudioError::API("401 Unauthorized".to_string());
        assert!(!manager.should_retry(&error));
    }

    #[test]
    fn test_should_retry_5xx() {
        let config = RetryConfig::default();
        let manager = RetryManager::new(config);
        
        let error = GeminiAudioError::API("500 Internal Server Error".to_string());
        assert!(manager.should_retry(&error));
    }

    #[test]
    fn test_retry_counter() {
        let config = RetryConfig::default();
        let mut manager = RetryManager::new(config);
        
        manager.increment_retry();
        assert_eq!(manager.get_retry_count(), 1);
        
        manager.reset_retries();
        assert_eq!(manager.get_retry_count(), 0);
    }
}
