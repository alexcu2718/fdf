
//use crate::direntry::DirEntry;

#[cfg(test)]
mod tests {
   // use super::*;
    use crate::direntry::DirEntry;
    use std::os::unix::ffi::OsStrExt;
    use tempfile::tempdir;

    #[test]
    fn test_file_type_checks() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("file.txt");
        std::fs::write(&file_path, "test").unwrap();
        
        let dir_path = temp_dir.path().join("subdir");
        std::fs::create_dir(&dir_path).unwrap();
        
   
        
        let file_entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert!(file_entry.is_regular_file());
        assert!(!file_entry.is_dir());
        
        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        assert!(dir_entry.is_dir());
        assert!(!dir_entry.is_regular_file());
        
    }

    #[test]
    fn test_path_methods() {
        let temp_dir = tempdir().unwrap();
        let file_path = temp_dir.path().join("parent/child.txt");
        std::fs::create_dir_all(file_path.parent().unwrap()).unwrap();
        std::fs::write(&file_path, "test").unwrap();
        
        let entry = DirEntry::new(file_path.as_os_str()).unwrap();
        assert_eq!(entry.file_name(), b"child.txt");
        assert_eq!(entry.extension().unwrap(), b"txt");
        assert_eq!(
            entry.parent(),
            file_path.parent().unwrap().as_os_str().as_bytes()
        );
    }

    #[test]
    fn test_read_dir() {
        let temp_dir = tempdir().unwrap();
        let dir_path = temp_dir.path().join("testdir");
        std::fs::create_dir(&dir_path).unwrap();
        
        std::fs::write(dir_path.join("file1.txt"), "test1").unwrap();
        std::fs::write(dir_path.join("file2.txt"), "test2").unwrap();
        std::fs::create_dir(dir_path.join("subdir")).unwrap();
        
        let dir_entry = DirEntry::new(dir_path.as_os_str()).unwrap();
        let entries = dir_entry.read_dir().unwrap();
        
        assert_eq!(entries.len(), 3);
        let mut names: Vec<_> = entries.iter().map(|e| e.file_name().to_vec()).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                b"file1.txt".to_vec(),
                b"file2.txt".to_vec(),
                b"subdir".to_vec()
            ]
        );
        
        for entry in entries {
            assert_eq!(entry.depth(), 1);
            assert_eq!(entry.base_len() as usize, dir_path.as_os_str().len() + 1);
        }
    }

    #[test]
    fn test_hidden_files() {
        let temp_dir = tempdir().unwrap();
        let hidden_file = temp_dir.path().join(".hidden");
        std::fs::write(&hidden_file, "").unwrap();
        
        let entry = DirEntry::new(hidden_file.as_os_str()).unwrap();
        assert!(entry.is_hidden());
        
        let non_hidden = temp_dir.path().join("visible");
        std::fs::write(&non_hidden, "").unwrap();
        let entry = DirEntry::new(non_hidden.as_os_str()).unwrap();
        assert!(!entry.is_hidden());
    }



   
}