+++
date = '2026-10-01T15:33:56+08:00'
draft = false
title = '嵌入式Linux学习02'
+++
# 嵌入式Linux学习02
## ioctl
### 1.1 为什么需要ioctl
假设我们的字符设备可能是一个简单的显示数字的装置。 此时我们需要设定和读取上面的数字。

固然我们可以通过write来进行数据的传递比如传递一个字符串`SET_DISPLAY 007`，然后在write里面进行解析。然而读取数字的时候则可能需要发送`GET_DISPLAY`，然后再调用`read`去读取设备传回来的数字。

这样的操作虽然可行，但会带来一个问题`write`和`read`原本更适合承担连续的数据传输，而像“设置数字”“读取数字”这些操作本质更接近控制命令。如果所有的控制操作都通过字符串协议塞进`write/read`中，不仅需要进行额外的命令解析，还需要自定义请求和响应流程，接口也会变得越来越复杂。


因此，Linux提供了一个`ioctl`作为专门的设备控制接口。通过`ioctl`，用户态程序可以直接向驱动发送明确的控制命令，并根据需要附带参数和返回值。

### 1.2 ioctl如何运作

```c
static long mydev_ioctl(
    struct file *file,
    unsigned int cmd,
    unsigned long arg)
{
    switch (cmd){
        //....
    }
    return 0;
}

static const struct file_operations mydev_fops = {
    //...
    .unlocked_ioctl = mydev_ioctl,
};
```

这样我们可以通过用类似`#define SET_DISPLAY 0x12345`之类的方式去定义不同数字代表着什么命令然后在switch里面匹配到对应的分支，但是Linux给了一套关于cmd如何定义的规范提供了四个宏。

| 名称 | 用法 | 作用 |
|---|---|---|
| `_IO` | `#define MYDEV_RESET _IO(MYDEV_MAGIC, 0)` | 不携带额外数据，只发送一个控制命令，例如复位、启动、停止 |
| `_IOW` | `#define MYDEV_SET_VALUE _IOW(MYDEV_MAGIC, 1, int)` | `int` 数据从用户态流向内核，`W` 表示用户向设备“写”数据，通常配合 `copy_from_user()` |
| `_IOR` | `#define MYDEV_GET_VALUE _IOR(MYDEV_MAGIC, 2, int)` | `int` 数据从内核流向用户态，`R` 表示用户从设备“读”数据，通常配合 `copy_to_user()` |
| `_IOWR` | `#define MYDEV_EXCHANGE _IOWR(MYDEV_MAGIC, 3, struct my_data)` | 数据双向流动，用户先传入数据，内核处理后再将结果写回用户态，通常同时使用 `copy_from_user()` 和 `copy_to_user()` |

> `W` 和 `R` 的方向是站在用户态程序的角度定义的。  
> `_IOW`、`_IOR`、`_IOWR` 这些宏本身只负责生成 ioctl 命令编号，并不会自动完成数据拷贝。真正的数据传输仍需要使用 `copy_from_user()` 和 `copy_to_user()`。

这些宏是如何生成出数字的呢。

在LINUX与内核侧相关的头文件中他们是这样定义的。
```c
#define _IO(type,nr)			    _IOC(_IOC_NONE,(type),(nr),0)
#define _IOR(type,nr,argtype)		_IOC(_IOC_READ,(type),(nr),(_IOC_TYPECHECK(argtype)))
#define _IOW(type,nr,argtype)		_IOC(_IOC_WRITE,(type),(nr),(_IOC_TYPECHECK(argtype)))
#define _IOWR(type,nr,argtype)		_IOC(_IOC_READ|_IOC_WRITE,(type),(nr),(_IOC_TYPECHECK(argtype)))


#define _IOC(dir,type,nr,size) \
	(((dir)  << _IOC_DIRSHIFT) | \
	 ((type) << _IOC_TYPESHIFT) | \
	 ((nr)   << _IOC_NRSHIFT) | \
	 ((size) << _IOC_SIZESHIFT))

//检查类型是否合法并最终返回需要传输的类型大小
#define _IOC_TYPECHECK(t) \
	((sizeof(t) == sizeof(t[1]) && \
	  sizeof(t) < (1 << _IOC_SIZEBITS)) ? \
	  sizeof(t) : __invalid_size_argument_for_IOC)
```
通过`_IOC`的位偏移可以看出这些宏的作用是把`unsigned int`分开几段分别编码了数据流向，类型，编号，和数据大小四类信息。

信息如何传递。最终还是通过`copy_from_user`和`copy_to_user`。
例如可以
```c
//...
case MYDEV_SET_VALUE:
    {
        if (copy_from_user(&mydev_value, (int __user *)arg, sizeof(int))) {
            return -EFAULT;
        }
        pr_info("mydev: ioctl SET_VALUE %d\n", mydev_value);
    }
    break;
//...
```
其中`arg`是需要写入的`int`类型在用户空间中的地址通过调用ioctl的时候传入变成了`unsigned long`。此时需要用`(int __user *)`将arg转回成用户控件中的地址。
>tips:具体行为还是通过代码来约定并不总是指针。


>读取示例
>```c
>case MYDEV_GET_VALUE:
>    {
>        if (copy_to_user((int __user *)arg, &mydev_value, sizeof(int))) {
>            return -EFAULT;
>        }
>        pr_info("mydev: ioctl GET_VALUE %d\n", mydev_value);
>    }
>    break;
>```
### 1.3 用户态如何使用ioctl
将以上代码编译`insmod`之后自然想开始进行控制。

很简单通过`open`取得fd之后。再通过
```c
ioctl(fd, cmd, arg);
```
其中在内核使用`_IO`等一系列宏生产的cmd需要同样的在用户程序中写好。

## blocking-io
### 2.1 blocking-io
在使用 `read` 读取设备数据时，设备并不一定始终处于可读状态，有时需要等待硬件或驱动完成数据准备。

如果用户程序不断通过 while 循环轮询设备状态，就会造成 CPU 持续空转，浪费大量计算资源。因此，更合理的方式是在暂时没有数据可读时让当前线程进入休眠状态；当设备数据准备完成后，再由内核将等待中的线程唤醒并继续执行读取操作。
### 2.2 如何实现
```c
static DECLARE_WAIT_QUEUE_HEAD(mydev_wait);
static bool data_ready = false;
```
在写入完成数据后可以用
```c
data_ready = true;
wake_up_interruptible(&mydev_wait);
```
读取数据时
```c
if (!data_ready){
    int ret = wait_event_interruptible(mydev_wait, data_ready);
    if (ret < 0 ){
        return ret; 
    }
}
```
等待完成

假设`read/write`都操作同一个`static char messgae[100]`。使用`cat /dev/mydev`的时候会等待调用write之后才会有输出。

### 2.3 wait_queue
在上面代码中使用了一个宏来定义了一个wait_queue`mydev_wait`。
这其中发生了什么?
```c
#define DECLARE_WAIT_QUEUE_HEAD(name) \
	struct wait_queue_head name = __WAIT_QUEUE_HEAD_INITIALIZER(name)

#define __WAIT_QUEUE_HEAD_INITIALIZER(name) {				\
	.lock		= __SPIN_LOCK_UNLOCKED(name.lock),			\
	.head		= LIST_HEAD_INIT(name.head) }
```

mydev_wait数据结构大致为
```c
struct wait_queue_head {
    spinlock_t lock;
    struct list_head head;
};

struct list_head {
    struct list_head *next;
    struct list_head *prev;
};
```
> `spinlock_t lock;`作用暂时理解为不能并发访问修改wait_queue。

### 2.4 `wake_up_interruptible`和`wait_event_interruptible`
```c
#define wait_event_interruptible(wq_head, condition)	
```
`wait_event_interruptibe`会condition为flase的时候，为当前任务创建一个等待节点加入`wq_head`对应的wait_queue当中。同时将当前任务设置为`TASK_INTERRUPTIBLE`状态并进入睡眠。任务被唤醒后会再次检查`condition`只有为ture时候才会继续执行。


而`wake_up_interruptible`会在wait_queue中遍历等待节点，并尝试唤醒处于`TASK_INTERRUPTIBLE`状态的任务并唤醒。
> 实际还有更复杂的机制。
### 2.5 poll and epoll
#### 2.5.1 在内核中支持poll
```c
static __poll_t mydev_poll(struct file *file, struct poll_table_struct *wait)
{
    __poll_t mask = 0;

    poll_wait(file, &mydev_wait, wait);

    if (data_ready) {
        mask |= POLLIN | POLLRDNORM; // Data is available to read
    }

    return mask;
}

static const struct file_operations mydev_fops = {
//....
    .poll    = mydev_poll,
};
```

这是什么?

在用户态程序中假设有
```c
read(fd1, buf1, sizeof(buf1)-1);
read(fd2, buf2, sizeof(buf2)-1);
read(fd3, buf3, sizeof(buf3)-1);
```
在`read`fd1的时候陷入阻塞的话程序没法执行后续的操作，但是后续的`read`并不依赖第一次`read`的结果。可以希望哪一个已经处于可以read的状态就去read，因此`poll`出现了。

#### 2.5.2 mydev_poll干了什么?
在`poll_wait`中会在mydev_wait增加一个节点，当这个节点被唤醒的时候会发送一个通知回到`struct poll_table_struct *wait)`。

然后如果数据准备好了`data_ready == true`。那么就会返回`mask |= POLLIN | POLLRDNORM;`。这两个位信息,`POLLIN`代表有数据可以读取，`POLLRDNORM`代表有普通数据可以读取。


#### 2.5.3 用户态怎么使用
可以通过
```c
    int fd = open("/dev/mydev", O_RDWR);
    if (fd < 0) {
        perror("Failed to open /dev/mydev");
        return 1;
    }
    struct pollfd fds[2];
    fds[0].fd = fd;
    fds[0].events = POLLIN;
    fds[1].fd = STDIN_FILENO;
    fds[1].events = POLLIN;
```
创建一个简单的poll。其中0位置上监听`mydev`，1位置上监听标准输入。`events = POLLIN`代表唤醒条件是要处于`POLLIN`。

当运行`poll(fds, 2, -1);`的时候
> int poll(struct pollfd *fds, nfds_t nfds, int timeout);
> 
> 分别对应pollfd和pollfd有效fd数量以及超时时间单位ms。
> 
> -1代表无限等待0代表不等待。

会遍历数组中每一个`fd`。并且找到对应的`file`调用`poll`函数。

如果全部fd都返回0那么则陷入休眠，直到的对应的`fd`有数据唤醒poll。然后会再次遍历数组中的`fd`，检查是否返回所需要的`mask`。

如果成功返回则继续执行。所以可以写出如下代码。
```c
    while(1){
        poll(fds, 2, -1);
        for(int i=0;i<2;i++){
            if( fds[i].revents & POLLIN){
                char buf[100];
                ssize_t n = read(fds[i].fd, buf, sizeof(buf)-1);
                if(n>0){
                    buf[n] = '\0';
                    printf("Read from fd %d: %s\n", fds[i].fd, buf);
                }else{
                    printf("Read error on fd %d\n", fds[i].fd);
                }
            }
        }
    }
```

可以用`tee`向/dev/mydev输入内容。而标准输入则是在当前程序运行框加入即可。

>makefile可以修改成如下多编译一个测试程序。
>```makefile
>obj-m += mydev.o
>
>KDIR := /lib/modules/$(shell uname -r)/build
>PWD := $(shell pwd)
>
>all:
>	$(MAKE) -C $(KDIR) M=$(PWD) LLVM=1 modules
>	clang test.c -o test  #加入内容
>clean:
>	$(MAKE) -C $(KDIR) M=$(PWD) LLVM=1 clean
>	rm -f test            #加入内容
>```


##### 2.5.4 epoll
epoll机制和poll十分类似。直接贴出等效代码
```c
#include <stdio.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/epoll.h>

int main()
{
    int fd = open("/dev/mydev", O_RDWR);
    if (fd < 0) {
        perror("Failed to open /dev/mydev");
        return 1;
    }

    int epfd = epoll_create1(0);
    if (epfd < 0) {
        perror("epoll_create1");
        close(fd);
        return 1;
    }

    struct epoll_event ev;

    ev.events = EPOLLIN;
    ev.data.fd = fd;

    if (epoll_ctl(epfd, EPOLL_CTL_ADD, fd, &ev) < 0) {
        perror("epoll_ctl mydev");
        close(epfd);
        close(fd);
        return 1;
    }

    ev.events = EPOLLIN;
    ev.data.fd = STDIN_FILENO;

    if (epoll_ctl(epfd, EPOLL_CTL_ADD, STDIN_FILENO, &ev) < 0) {
        perror("epoll_ctl stdin");
        close(epfd);
        close(fd);
        return 1;
    }

    struct epoll_event events[2];

    while (1) {
        int n = epoll_wait(epfd, events, 2, -1);

        if (n < 0) {
            perror("epoll_wait");
            break;
        }

        for (int i = 0; i < n; i++) {
            if (events[i].events & EPOLLIN) {
                char buf[100];

                ssize_t len = read(
                    events[i].data.fd,
                    buf,
                    sizeof(buf) - 1
                );

                if (len > 0) {
                    buf[len] = '\0';

                    printf(
                        "Read from fd %d: %s\n",
                        events[i].data.fd,
                        buf
                    );
                } else if (len == 0) {
                    printf(
                        "EOF on fd %d\n",
                        events[i].data.fd
                    );
                } else {
                    perror("read");
                }
            }
        }
    }

    close(epfd);
    close(fd);

    return 0;
}
```

与poll相比epoll会记录有多少个fd已经准备好了，并且放入通过`epoll_wait`传入的events中直接返回准备好的fd个数。

与poll相比，不用再遍历全部的fd了，假设在poll的数量非常大的情况下使用epoll能显著提升性能。

