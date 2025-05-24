# 终于写完了ch6。
## 实现的功能
### fstat
    - 实现思路
    - 根据_fd在fd_table中找到对应的OSInode。我修改了File trait，增加了fstat的方法，这样就可以直接调用file.fstat(),而不用苦恼于如何把获得的Arc<dyn File + Send + Sync + 'static>类型的file转换为OSInode。
    - 修改为OSInode实现的File trait，即实现fstat方法：调用对应的Inode的get_stat(这个也是新实现的，inode_id就是根据节点的block_id和block_offset逆向算出来的)方法，获得最重要的inode_id和link_count；调用对应的Inode的inode_type(新实现的方法)，根据结果构建StatMode字段。
    - 为什么不让Inode的get_stat方法直接返回一个Stat类型的值？因为我不知道怎么让vfs.rs使用os/src/fs/inode.rs中定义的Stat和StatMode结构。。。。。
    
### linkat
    - 实现思路：根据old_name找到对应的OSInode，调用该OSInode中新增的link方法。
    - OSInode的link中，调用父目录（ROOT_INODE）的link方法。
    - Inode的link中，找到old_name对应的inode，调用其modify_disk_node方法，修改disk_node的link_count字段。
    *是不是如果我把link_count放在inode里就不用调那么多次disk_node了。。
    - 为自己（ROOT_INODE）增加一个目录项，name字段为new_name，inode_id为 old_name对应的inode。
    *在实现fstat的时候写了一个辅助方法，get_inode_id(放在easyfilesystem的impl里)
### unlinkat
    - 实现思路与linkat类似。

### 遇到的问题
    - 本地测试通过，ci测试时，ch6_file3执行到iteration8就会超时，然后被kill掉。
    - 解决方法：把write相关的block_cache_sync_all()，close里添加这个。不知道是否合理
## 问答作业
### 在我们的easy-fs中，root inode起着什么作用？如果root inode中的内容损坏了，会发生什么？
    - ROOT_INODE是所有文件的父目录，是文件系统的入口点。如果它的内容损坏了，那么所有其它inode都无法被访问到了（逻辑上的），虽然它们仍存在于文件系统中（物理上的）。

# ch7

## 问答作业
### 1 pipe管道的一个实际应用
    cat file.txt | wc -l 统计file.txt的行数，把左边的标准输出作为右侧的标准输入
    “|”是管道操作符
### 如果需要在多个进程间互相通信，则需要为每一对进程建立一个管道，非常繁琐，请设计一个更易用的多进程通信机制。
    可以建立一个类似于管道的机制，但是有多个写端和多个读端？
