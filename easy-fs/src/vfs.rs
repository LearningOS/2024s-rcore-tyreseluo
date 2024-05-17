use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};

/// Virtual filesystem layer over easy-fs
pub struct Inode {
    /// inode 文件所在 inode 编号
    pub ino: u64,
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        ino: u64,
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            ino,
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }
    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }
    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }
    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_id() as u32);
            }
        }
        None
    }
   
    /// Get the stat of a inode
    /// first element is inode number
    /// second element is mode
    /// third element is number of hard links
    pub fn stat(&self) -> (u64, &str, u32) {
        self.read_disk_inode(|disk_inode| {     
            let mode = if disk_inode.is_dir() {
                "DIR"
            } else if disk_inode.is_file() {
                "FILE"
            } else {
                "NULL"
            };

            (
                self.ino,
                mode,
                disk_inode.nlink
            )
        })
    }
    

    /// Link a inode under current inode by name
    pub fn link(&self, inode: Arc<Inode>, name: &str) -> isize {
        let mut fs = self.fs.lock();
        let mut error = 0;
        self.modify_disk_inode(|root_inode| {
            if self.find_inode_id(name, root_inode).is_some() {
                // file already exists
                // 为了方便，不考虑新文件路径已经存在的情况（属于未定义行为），除非链接同名文件。
                error = -1;
                return;
            }

            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            let dirent = DirEntry::new(name, inode.ino as u32);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        if error != 0 {
            return error;
        }

        // increase nlink
        inode.modify_disk_inode(|inode_dist| {
            inode_dist.nlink += 1;
        });

        0
    }


    /// 注意考虑使用 unlink 彻底删除文件的情况，此时需要回收inode以及它对应的数据块。
    /// Unlink a inode under current inode by name
    pub fn unlink(&self, inode: Arc<Inode>, name: &str) -> isize {
        {
            let mut fs = self.fs.lock();
            self.modify_disk_inode(|disk_inode| {
                assert!(disk_inode.is_dir());//目录不处理

                let file_count = (disk_inode.size as usize) / DIRENT_SZ;
                let mut ent_index = 0xffff_ffff;
                for i in 0..file_count {
                    let mut dirent= DirEntry::empty();
                    assert_eq!(
                        disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                        DIRENT_SZ,
                    );
                    if dirent.name() == name {
                        ent_index = i;
                        break;
                    }
                }

                if ent_index == 0xffffffff {
                    return;
                }

                for i in ent_index + 1..file_count {
                    let mut dirent = DirEntry::empty();
                    assert_eq!(
                        disk_inode.read_at(
                            i * DIRENT_SZ,
                            dirent.as_bytes_mut(),
                            &self.block_device
                        ),
                        DIRENT_SZ,
                    );
                    disk_inode.write_at((i - 1) * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
                }
                let new_size = (file_count - 1) * DIRENT_SZ;
                self.decrease_size(new_size as u32, disk_inode, &mut fs);
            });
        }

         // decrease link count
         let mut clear = false;
         inode.modify_disk_inode(|inode_disk| {
             inode_disk.nlink -= 1;
             clear = inode_disk.nlink == 0;
         });
         if clear {
             inode.clear();
             inode.fs.lock().dealloc_inode(inode.ino as u32);
         }
         0
    }

    /// Find inode under current inode by name
    /// 根据文件名查找对应的磁盘上的inode
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    inode_id as u64,
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }
    
   
    
    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }


     /// Decrease the size of a disk inode
     fn decrease_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size > disk_inode.size {
            return;
        }
        let blocks_dealloc = disk_inode.decrease_size(&self.block_device, new_size);
        for data_block in blocks_dealloc.into_iter() {
            fs.dealloc_data(data_block);
        }
    }

    /// Create inode under current inode by name 
    /// 在根目录下创建一个文件
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        let op = |root_inode: &DiskInode| {
            // assert it is a directory
            assert!(root_inode.is_dir());
            // has the file been created?
            self.find_inode_id(name, root_inode)
        };
        if self.read_disk_inode(op).is_some() {
            return None;
        }
        // create a new file
        // alloc a inode with an indirect block
        let new_inode_id = fs.alloc_inode();
        // initialize inode
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // return inode
        Some(Arc::new(Self::new(
            new_inode_id as u64,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }
    /// List inodes under current inode
    pub fn ls(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }
    /// Read data from current inode
    /// 根据inode找到文件数据所在的磁盘数据块，并读到内存中
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }
    /// Write data to current inode
    /// 根据inode找到文件数据所在的磁盘数据块，把内存中数据写入到磁盘数据块中
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }
    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
}
