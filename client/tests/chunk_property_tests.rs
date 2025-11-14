// Property-based tests for chunk processing
use cooklang_sync_client::chunker::{Chunker, InMemoryCache};
use proptest::prelude::*;
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

// Helper function to create test files
async fn create_test_file(dir: &TempDir, name: &str, content: &[u8]) {
    let mut file = File::create(dir.path().join(name)).await.unwrap();
    file.write_all(content).await.unwrap();
    file.flush().await.unwrap();
}

proptest! {
    #[test]
    fn test_chunk_hash_idempotency(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let cache = InMemoryCache::new(100, 10000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        let hash1 = chunker.hash(&data, 10);
        let hash2 = chunker.hash(&data, 10);

        // Same data should always produce same hash
        prop_assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_chunk_save_and_read_preserves_data(
        chunk1 in prop::collection::vec(any::<u8>(), 1..500),
        chunk2 in prop::collection::vec(any::<u8>(), 1..500)
    ) {
        let cache = InMemoryCache::new(1000, 100000);
        let mut chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        // Generate hashes
        let hash1 = chunker.hash(&chunk1, 16);
        let hash2 = chunker.hash(&chunk2, 16);

        // Save chunks
        chunker.save_chunk(&hash1, chunk1.clone()).unwrap();
        chunker.save_chunk(&hash2, chunk2.clone()).unwrap();

        // Read chunks
        let retrieved1 = chunker.read_chunk(&hash1).unwrap();
        let retrieved2 = chunker.read_chunk(&hash2).unwrap();

        // Verify data is preserved
        prop_assert_eq!(chunk1, retrieved1);
        prop_assert_eq!(chunk2, retrieved2);
    }

    #[test]
    fn test_hash_size_parameter(
        data in prop::collection::vec(any::<u8>(), 1..100),
        size in 4..32usize
    ) {
        let cache = InMemoryCache::new(100, 10000);
        let chunker = Chunker::new(cache, PathBuf::from("/tmp"));

        let hash = chunker.hash(&data, size);

        // Hash length should match requested size
        prop_assert_eq!(hash.len(), size);
    }
}

// Async property tests using tokio test runtime
#[cfg(test)]
mod async_property_tests {
    use super::*;

    #[tokio::test]
    async fn test_text_file_round_trip_various_sizes() {
        let test_cases = vec![
            ("Line1\n", 1),
            ("Line1\nLine2\n", 2),
            ("Line1\nLine2\nLine3\n", 3),
            ("A\nB\nC\nD\nE\n", 5),
        ];

        for (content, expected_chunks) in test_cases {
            let temp_dir = TempDir::new().unwrap();
            let cache = InMemoryCache::new(1000, 100000);
            let mut chunker = Chunker::new(cache, temp_dir.path().to_path_buf());

            let test_file = "test.cook";
            create_test_file(&temp_dir, test_file, content.as_bytes()).await;

            // Hashify the file
            let hashes = chunker.hashify(test_file).await.unwrap();

            // Verify number of chunks
            assert_eq!(
                hashes.len(),
                expected_chunks,
                "Failed for content: {:?}",
                content
            );

            // Reconstruct file
            let hash_refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
            let output_file = "output.cook";
            chunker.save(output_file, hash_refs).await.unwrap();

            // Verify content is preserved
            let reconstructed = tokio::fs::read(temp_dir.path().join(output_file))
                .await
                .unwrap();
            assert_eq!(
                reconstructed,
                content.as_bytes(),
                "Content mismatch for: {:?}",
                content
            );
        }
    }

    #[tokio::test]
    async fn test_binary_file_round_trip_various_sizes() {
        let test_cases: Vec<Vec<u8>> = vec![
            vec![1, 2, 3, 4, 5],                          // Small file
            vec![0u8; 1024],                              // 1KB file
            vec![255u8; 2048],                            // 2KB file
            (0..5000).map(|i| (i % 256) as u8).collect(), // 5KB file with pattern
        ];

        for content in test_cases {
            let temp_dir = TempDir::new().unwrap();
            let cache = InMemoryCache::new(10000, 10000000);
            let mut chunker = Chunker::new(cache, temp_dir.path().to_path_buf());

            let test_file = "test.jpg";
            create_test_file(&temp_dir, test_file, &content).await;

            // Hashify the file
            let hashes = chunker.hashify(test_file).await.unwrap();

            // Reconstruct file
            let hash_refs: Vec<&str> = hashes.iter().map(|s| s.as_str()).collect();
            let output_file = "output.jpg";
            chunker.save(output_file, hash_refs).await.unwrap();

            // Verify content is preserved
            let reconstructed = tokio::fs::read(temp_dir.path().join(output_file))
                .await
                .unwrap();
            assert_eq!(
                reconstructed,
                content,
                "Binary content mismatch for size: {}",
                content.len()
            );
        }
    }
}
