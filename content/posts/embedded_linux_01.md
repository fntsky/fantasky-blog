+++
date = '2026-09-29T11:52:16+08:00'
draft = false
title = '嵌入式linux学习01'
+++
## 嵌入式linux学习01

### 1.基本认识
#### fd是什么
fd 是 **file descriptor** 文件描述符，其本质是一个整数。
```c
int fd = open("/dev/mydev", O_RDWR);
/*
open函数第二个参数打开标志
O_RDONLY   // 只读
O_WRONLY   // 只写
O_RDWR     // 读写
*/
```
以上代码用于创建一个fd。

fd是只能作用在进程局部。A进程获取了一个fd = 3。在B进程并不能通过3访问到同样设备。
有了fd之后可以通过fd访问到对应设备。
```c
char buf[100];
ssize_t n = read(fd, buf, sizeof(buf));
```
当使用open打开设备的时候。内核会为这次打开建立相应的文件打开对象**struct file \***。进程中的fd table会保存指向该对象的引用。fd table本质是一个数组，fd是对应设备的下标。
>Linux 进程启动时，通常约定 fd 0、1、2 分别对应 stdin、stdout、stderr，但它们并不是不可改变的，可以被关闭、重定向或重新分配。


#### struct file ?
```c
struct file {
    struct path        f_path;
    const struct file_operations *f_op;

    loff_t             f_pos;
    unsigned int       f_flags;

    void              *private_data;

    ...
};
```

| 成员 | 作用|
|-----|-----|
|f_path| 指向文件路径|
|f_op|当前文件支持什么操作|
|f_pos|读写偏移|
|f_flags|打开标志|
|private_data|供驱动保存本次打开对应的私有数据|


### 什么是文件?
在 Linux 中有“一切皆文件”的设计哲学。设备驱动可以通过创建设备节点暴露到文件系统中，使用户空间能够像操作文件一样，通过 open、read、write 等接口访问设备。


一个什么样的文件是字符设备？
```shell
ls -l /dev/null
#输出crw-rw-rw- 1,3 root 29 9月  14:37 󰡯 /dev/null
```
> /dev/null 是一个非常特殊的字符设备。写入的字符流永远被丢弃。读取字节流永远EOF。

其中第一个c标识着/dev/null这个文件是一个字符设备(**character device**)。

常见文件类型
|首字符	|类型|
|------|---|
|-	|普通文件|
|d|	目录|
|c|	字符设备|
|b|	块设备|
|l|	符号链接|
|p|	FIFO/管道|
|s|	socket|

目前先讨论字符设备。

### 2.编写一个字符设备驱动程序
> 目标是编写一个可以加载进内核的驱动模块，注册一个字符设备，并在 /dev 下创建设备节点，使用户空间能够通过文件操作访问它。

```c
//mydev.c
#include<linux/kernel.h>
#include<linux/module.h>
#include <linux/init.h>


static int __init mydev_init(void)
{
    pr_info("MyDev Loaded!\n");
    return 0;
}

static void __exit mydev_exit(void)
{
    pr_info("mydev: unloaded\n");
}
//标注模块进入和退出时候的函数
module_init(mydev_init);
module_exit(mydev_exit);
//以下声明模块信息
MODULE_LICENSE("GPL");
MODULE_AUTHOR("your_name");
MODULE_DESCRIPTION("My first Linux kernel module");
```

然后编写Makefile

```Makefile
obj-m += mydev.o

KDIR := /lib/modules/$(shell uname -r)/build
PWD := $(shell pwd)

all:
	$(MAKE) -C $(KDIR) M=$(PWD) LLVM=1 modules

clean:
	$(MAKE) -C $(KDIR) M=$(PWD) LLVM=1 clean
```
> 如果使用GCC LLVM=1可以去掉

$(MAKE) 调用 make，-C $(KDIR) 让 make 切换到当前内核的构建目录，并使用内核的 Kbuild 系统进行模块编译。

该makefile主要是通过内核自己的编译程序进行编译。

使用`bear -- make`可以顺便生成clangd支持的`compile_commands.json`

成功运行之后可以看到一个`mydev.ko`。
使用`sudo insmod mydev.ko`可以加载进模块里面。
使用`sudo rmmod mydev`可以卸载。
使用`sudo dmesg | tail` 可以看到属于mydev的输出。

现在`mydev.c`只是一个毫无作用的程序。下一步会实现注册设备号。

> 设备号是一个本质是一个整数。类型在内核中是`dev_t`。
> 设备号有主设备号和次设备号在代码中通过`MAJOR(dev_num)`和`MINOR(dev_num)`获取。

在代码中可以通过`alloc_chrdev_region`来申请一个设备号
```c
static dev_t dev_num;

int ret = alloc_chrdev_region(&dev_num, 0, 1, "mydev");
/*int alloc_chrdev_region(dev_t *dev,       设备号保存地方
                        unsigned baseminor, 从哪个次设备号开始
                        unsigned count,     连续多少个设备号
                        const char *name);  设备名字
返回值0 代表成功
返回负的错误码表示失败
*/
```
然后将申请代码添加进入`mydev_init`中。
```c
static int __init mydev_init(void)
{
    pr_info("MyDev Loaded!\n");
    int ret = alloc_chrdev_region(&dev_num, 0, 1, "mydev");
    if (ret < 0) {
        pr_err("Failed to allocate char device region\n");
        return ret;
    }
}
static void __exit mydev_exit(void)
{
    //在推出函数中取消申请即可
    unregister_chrdev_region(dev_num, 1);
    pr_info("mydev: unloaded\n");
}
```

还需要初始化一个字符设备对象`struct cdev`
> cdev承载着真正的字符设备，解决访问这个字符设备的时候如何处理。

```c
static struct cdev my_cdev;
static const struct file_operations mydev_fops;


cdev_init(&my_cdev, &mydev_fops);
```

`file_operations`最终决定了这个字符设备支持什么操作。

接下来实现两个简单的读写操作。
```c
static char message[100] = "Hello from mydev!\n";

static ssize_t mydev_read(
    struct file *file,    //当前打开的文件对象
    char __user *buf,     //用户空间缓冲区，用来接收驱动读取的数据
    size_t count,         //用户希望读取的字节数
    loff_t *ppos)         //当前文件偏移位置，读取后通常需要更新
{
    size_t len = strlen(message);
    if (*ppos >= len) {
        return 0; // EOF
    }
    if (count > len - *ppos) {
        count = len - *ppos;
    }
    if (copy_to_user(buf, message + *ppos, count)) {
        return -EFAULT;
    }
    *ppos += count;
    return count;
}


static ssize_t mydev_write(
    struct file *file,      //当前打开的文件对象
    const char __user *buf, //用户空间缓冲区，保存用户希望写入驱动的数据
    size_t count,           //用户希望写入的字节数
    loff_t *ppos)           //当前文件偏移位置，写入后通常需要更新
{
    char kbuf[100];
    size_t len;
    len = min(count, sizeof(kbuf) - 1);
    if (copy_from_user(kbuf, buf, len)) {
        return -EFAULT;
    }
    kbuf[len] = '\0';
    pr_info("mydev: write %zu bytes: %s\n", len, kbuf);
    return count;
}

//暂时不实现
static int mydev_open(struct inode *inode, struct file *file)
{
    pr_info("mydev: opened\n");
    return 0;
}


//暂时不实现
static int mydev_release(struct inode *inode, struct file *file)
{
    pr_info("mydev: released\n");
    return 0;
}

static const struct file_operations mydev_fops = {
    .owner   = THIS_MODULE,
    .open    = mydev_open,
    .release = mydev_release,
    .write   = mydev_write,
    .read    = mydev_read,
};
```
接下来就可以实现完整init了。
```c
static int __init mydev_init(void)
{
    pr_info("MyDev Loaded!\n");
    int ret = alloc_chrdev_region(&dev_num, 0, 1, "mydev");
    if (ret < 0) {
        pr_err("Failed to allocate char device region\n");
        return ret;
    }

    cdev_init(&my_cdev, &mydev_fops);

    //将cdev和dev_num关联起来。
    ret = cdev_add(&my_cdev, dev_num, 1);
    if (ret < 0) {
        pr_err("Failed to add cdev\n");
        unregister_chrdev_region(dev_num, 1);
        return ret;
    }

    pr_info("Allocated char device region: Major %d, Minor %d\n", 
        MAJOR(dev_num), 
        MINOR(dev_num));
    
    return 0;
}

//也要释放
static void __exit mydev_exit(void)
{
    cdev_del(&my_cdev);
    unregister_chrdev_region(dev_num, 1);
    pr_info("mydev: unloaded\n");
}
```

重新编译insmod后使用dmesg查看一下
```
[10421.907101] Allocated char device region: Major 507, Minor 0
```
使用命令创建设备节点`sudo mknod /dev/mydev c 507 0`。最后两个数字就是主设备号和次设备号。
>507 是本次动态分配得到的主设备号，并不是固定值。模块重新加载后可能得到不同的主设备号，因此应以实际输出为准。
```shell
$ cat /dev/mydev
Hello from mydev!
$ echo "hello kernel" | sudo tee /dev/mydev  
$ sudo dmesg | tail
[10665.545782] mydev: write 13 bytes: hello kernel
```

至此成功完成了一个内核模块，可以注册一个设备进行简单的读写操作。


finally
```c
#include<linux/kernel.h>
#include<linux/module.h>
#include <linux/init.h>
#include <linux/fs.h>
#include <linux/cdev.h>
#include <linux/uaccess.h>

static dev_t dev_num;

static struct cdev my_cdev;

static char message[100] = "Hello from mydev!\n";

static ssize_t mydev_read(
    struct file *file, 
    char __user *buf, 
    size_t count, 
    loff_t *ppos)
{
    size_t len = strlen(message);
    if (*ppos >= len) {
        return 0; // EOF
    }
    if (count > len - *ppos) {
        count = len - *ppos;
    }
    if (copy_to_user(buf, message + *ppos, count)) {
        return -EFAULT;
    }
    *ppos += count;
    return count;
}

static ssize_t mydev_write(
    struct file *file, 
    const char __user *buf, 
    size_t count, 
    loff_t *ppos)
{
    char kbuf[100];
    size_t len;
    len = min(count, sizeof(kbuf) - 1);
    if (copy_from_user(kbuf, buf, len)) {
        return -EFAULT;
    }
    kbuf[len] = '\0';
    pr_info("mydev: write %zu bytes: %s\n", len, kbuf);
    return count;
}


static int mydev_open(struct inode *inode, struct file *file)
{
    pr_info("mydev: opened\n");
    return 0;
}

static int mydev_release(struct inode *inode, struct file *file)
{
    pr_info("mydev: released\n");
    return 0;
}

static const struct file_operations mydev_fops = {
    .owner   = THIS_MODULE,
    .open    = mydev_open,
    .release = mydev_release,
    .write   = mydev_write,
    .read    = mydev_read,
};




static int __init mydev_init(void)
{
    pr_info("MyDev Loaded!\n");
    int ret = alloc_chrdev_region(&dev_num, 0, 1, "mydev");
    if (ret < 0) {
        pr_err("Failed to allocate char device region\n");
        return ret;
    }

    cdev_init(&my_cdev, &mydev_fops);

    ret = cdev_add(&my_cdev, dev_num, 1);
    if (ret < 0) {
        pr_err("Failed to add cdev\n");
        unregister_chrdev_region(dev_num, 1);
        return ret;
    }

    pr_info("Allocated char device region: Major %d, Minor %d\n", 
        MAJOR(dev_num), 
        MINOR(dev_num));
    
    return 0;
}

static void __exit mydev_exit(void)
{
    cdev_del(&my_cdev);
    unregister_chrdev_region(dev_num, 1);
    pr_info("mydev: unloaded\n");
}

module_init(mydev_init);
module_exit(mydev_exit);


MODULE_LICENSE("GPL");
MODULE_AUTHOR("yourname");
MODULE_DESCRIPTION("My first Linux kernel module");
```