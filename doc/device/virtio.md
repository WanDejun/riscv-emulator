# VirtIO

## 映射方式

1. PortIO
2. MMIO(riscv-system采用)
3. PCIe map

## 通讯

通讯是通过 `VirtQueue` 作为 channel 实现的, 本质是一个无锁队列. 事件机制为:  
1. driver -> device: 写入 `QueueNotify = 0x050,` 寄存器, 触发写入事件回调.
2. device -> driver: 发送中断.

### VirtQueue

包含 `DescriptionTable`, `AvailRing`, `UsedRing` 三个部分. `DescriptionTable` 采用被作为链表空间使用.  

#### 初始化

分配三个部分的空间, 将各自的起始地址以长度告知 `device`

#### `driver` 发送请求

其中 driver 给 device 发送请求: 
1. 一个请求可能包含多个描述, 每个描述占用一个 `DescriptionTable` 项, 组织成链表形式. 因此根据需要分配并填写 `DescriptionTable` 项.  
2. 分配 `addr` 用于传递数据(写入 `blk` 的数据 / 从 `blk` 读取的数据).
3. 从 `AvailRing` 中取出一个可用节点.
4. 将 `escriptionTable` 链表头的地址放入 `AvailRing` 中取出的节点.
5. 重复 *1-4* 的操作直至没有后续事务.
6. 向 `QueueNotify = 0x050` 寄存器写入一个数据, 通知 `virt device` 处理.
7. (可能) 等待中断, 从 `UsedRing` 中取数据 (一般是请求的状态: 成功/失败, 实际读写数量)
8. 若为读操作, 从 `addr` 中取出数据使用.
