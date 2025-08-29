# 送给 QUIC 优雅重启的一颗子弹：浅谈 Cloudflare udpgrm

0.优雅重启 & 背景
At Cloudflare, we do everything we can to avoid interruption to our services. We frequently deploy new versions of the code that delivers the services, so we need to be able to restart the server processes to upgrade them without missing a beat. In particular, performing graceful restarts (also known as "zero downtime") for UDP servers has proven to be surprisingly difficult.

翻译：在 Cloudflare，我们竭尽所能避免服务中断。我们频繁对现有服务的代码进行变更，因此我们需要能够重启服务器进程以进行升级，而我们不希望这会导致任何服务中断。值得指出的是，对 UDP 服务器进行优雅重启（也就是所谓的"零停机时间"）已被证明极其困难。
我们已经研究过若干 TCP 优雅重启的方案，比如这里介绍了若干种 case 来帮助我们管理 TCP 的优雅重启。笔者在这里不会花大量的篇幅来全部介绍，让我们来挑选 Nginx 的优雅重启的例子，来帮助我们简单理解可以怎么做：

nginx 会使用一个 master 来管理若干个 worker 进程。而这若干个 worker 进程才会真正执行操作，master 进程则负责协调。这个模型很普遍了，比如 raft 就是类似的。
而所有的 listen socket 都会被 master 进程管理，而 worker 进程则负责处理这些 listen socket 上的连接。
当需要优雅重启的时候，我们先发送一个 USR2 信号给 master 进程，这会开启一个新的 master 进程（通过 exec），然后让这个新的 master 进程来获取现有的信息，再让这个新的 master 进程来开启若干新的 worker 进程。
当一个新的连接请求到达内核时，内核会从所有正在监听这个套接字的进程中（所有老的和新的 worker）选择一个来处理请求。这保证了新连接不会被拒绝。
一旦新的 worker 全部启动并准备就绪，老的 master 进程会向它的 worker 们发送一个优雅关闭的信号（QUIT）。在确定收到信号后，老的 worker 会逐步下线（这包含了停止接受新的连接，继续处理手中所有已经建立的连接等）。这些都解决了之后才会退出进程。
最后的最后，现在我们只剩下了新的 master 进程，它将开始处理新的连接请求。
当然这都是最简单的基础模型，我们在实践中会有很多细节可以优化，比如：

我们用 systemd 来让套接字的生命周期和应用的生命周期解耦。
信号在这里是不好的，因为缺乏反馈。我们可以使用一个 Unix socket 来管理 master 进程和 worker 进程的通信。参考这里。
但无论怎么说，这个模型很好，适合我们初步的理解。那我们仍然可以在 UDP 上进行类似的优雅重启吗？

1.有状态 UDP 的阿喀琉斯之踵
上面这个问题的答案是：可以，但是也不可以。

显然的，nginx 是一个无状态的代理，所以这么做并没有问题，你对于连接的任何状态不需要关心。但是假设你在使用 QUIC，WireGuard 和 SIP 这样的协议或者正在制作一个在线游戏，那么，你需要使用有状态流(stateful flow) 来维护。让我们思考一下，当服务器进程重启时，与流相关的状态会发生什么变化呢？通常，服务器重启期间会直接断开旧连接。将流状态从旧实例迁移到新实例是可能的，但这很复杂，而且众所周知很难做到正确。

TCP 连接也存在同样的问题，但一种常见的方法是让服务器进程的旧实例与新实例同时运行一段时间，将新连接路由到新实例，同时让现有连接消耗旧实例的连接。一旦所有连接完成或达到超时，就可以安全关闭旧实例。具体地说，旧服务器进程会停止 accept() 新连接，只是等待旧连接逐渐消失。

同样的方法也适用于 UDP，但与 TCP 相比，它需要服务器进程的更多参与（显然 UDP 不能 accept()）。TCP 本身就是面向连接的，而 QUIC 之流则需要我们在协议侧做更多的努力。

2. Established-over-unconnected technique
对于某些服务，我们可以使用一种称为"established-over-unconnected"的技术。这个技术的发端是，我们希望在 Linux 上实现在未连接的 socket 上创建已经连接的 socket。

我知道上面这段话看起来太像是绕口令了，所以你没看懂的话，我们直接从代码开始理解好了：

sd = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
# SO_REUSEADDR 允许多个套接字绑定到同一个地址和端口
sd.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
# 请求内核在接收数据包时，提供额外（cmsg）的目的地信息
sd.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_RECVPKTINFO, 1)

sd.bind((host, port))
buf, cmsg, flags, remote_addr = sd.recvmsg(2048, 1024)
local_addr = unpack_cmsg(cmsg)

#...

cd = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)

# 同样设置 SO_REUSEADDR 选项
cd.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
# 将新套接字也绑定到和 sd 完全相同的本地 IP 和端口
cd.bind(local_addr)
cd.connect(remote_addr)
这段代码里我们在做的是： - 我们启动一个 UDP 未连接套接字。

等待 client 连接。
一旦 client 连接，我们创建一个全新的 UDP 套接字，并绑定到 client 的地址。
在 ss 里面查看一下。

$ ss -panu sport = :1234 or dport = :1234 | cat
State     Recv-Q    Send-Q       Local Address:Port        Peer Address:Port    Process
ESTAB     0         0                    [::1]:1234               [::1]:44592    python3
UNCONN    0         0                        *:1234                   *:*        python3
ESTAB     0         0                    [::1]:44592              [::1]:1234     nc
总而言之，我们可以用这种黑魔法，从而让我们能在 UDP 上实现类似 TCP 的 accept()。每个入口连接都有自己的专用的 socket fd。

读到这里肯定会有大神思考到，这里代码仍然有一些小缺陷，比如：

Client 发送给 sd 第一个 packet 之后，同时很快地发送了几个 packet（或者首包和后面的包一起到达了），但是 cd 暂时还没有准备好，那这几个 packet 还需要再缓存再等待 cd 准备好之后再发送。
cd.bind 和 cd.connect 之间会有一个时间窗口，对于海量连接来说这是致命的。因为在 bind 之后我们可以接受到任何 client 的 packet，而只有在 connect 之后我们只能接受到指定 client 发给我们的 packet。我们必须在这个时间窗口内做一些过滤不至于出现大量错误路由的 packet。
如果有海量的连接下，由于 kernel 内使用哈希表来管理 socket fd，且我们这里的 sd 和若干的 cd 都绑定到一个 IP:Port 的 Key 上，这里会形成一个非常长的链挂在这个 Key 上。显然在大量的连接后，我们会面对查找退化到 O(n) 的问题，性能雪崩。
不过没关系，现在这个代码还很简单，只是试试水测试一下大概的思路。我们现在应该心里有数，在 UDP 上实现优雅重启是可以做到的。

3. SO_REUSEPORT is all you need(1/2)
让我们先回顾一点有趣的基础知识。大家往往都知道 SO_REUSEADDR，但是 SO_REUSEPORT 呢？读过一句话很有趣，SO_REUSEPORT 才是大家期望的 SO_REUSEADDR。

Basically, SO_REUSEPORT allows you to bind an arbitrary number of sockets to exactly the same source address and port as long as all prior bound sockets also had SO_REUSEPORT set before they were bound. If the first socket that is bound to an address and port does not have SO_REUSEPORT set, no other socket can be bound to exactly the same address and port, regardless if this other socket has SO_REUSEPORT set or not, until the first socket releases its binding again. Unlike in case of SO_REUSEADDR the code handling SO_REUSEPORT will not only verify that the currently bound socket has SO_REUSEPORT set but it will also verify that the socket with a conflicting address and port had SO_REUSEPORT set when it was bound.
简单来说，SO_REUSEPORT 允许你绑定多个 socket 到同一个 IP:Port 上，只要这些 socket 都设置了 SO_REUSEPORT。

它有什么用？我们通常把它用于负载均衡，使服务器能够高效地在多个 CPU 核心之间分配流量。您可以将其视为将一个 IP:port 与多个数据包队列(packet queue) 关联起来的一种方式。在内核中，以这种方式共享 IP:port 的套接字被组织成一个 reuseport 组——我们将在本文中频繁提到这个术语。

┌───────────────────────────────────────────┐
│ reuseport group 192.0.2.0:443             │
│ ┌───────────┐ ┌───────────┐ ┌───────────┐ │
│ │ socket #1 │ │ socket #2 │ │ socket #3 │ │
│ └───────────┘ └───────────┘ └───────────┘ │
└───────────────────────────────────────────┘
Linux 支持多种方法在 reuseport 组之间分发入站数据包。默认情况下，内核使用数据包四元组的哈希值来选择目标 socket，虽然这个方法可以保证一个完整的会话中的所有数据包都会被稳定地、持续地发送到同一个进程 /socket，但是对于 CPU 缓存不友好，举个例子，由于哈希，很有可能我们在 CPU0 上接受后，需要转发到 CPU4。这样会导致 cpu cache miss。另一种方法是 SO_INCOMING_CPU ，启用后，它会尝试将数据包引导至与接收数据包的 CPU 相同的 socket。这种方法有效，但灵活性有限,因为我们有可能会把大量数据包都 sharding 到几个核心上，而其他闲置的 CPU 核心会浪费。

4. eBPF is all you need(2/2)
为了提供更多控制，Linux 引入了 SO_ATTACH_REUSEPORT_CBPF 选项，允许服务器进程附加一个 Classical BPF (cBPF) 程序来做出套接字选择决策。该选项后来被扩展为 SO_ATTACH_REUSEPORT_EBPF ，从而支持使用现代 eBPF 程序。借助 eBPF ，开发人员可以实现很多任意的自定义逻辑（前提是在内核限制内）。一个 DEMO 如下所示：

SEC("sk_reuseport")　// hook 到内核的 sk_reuseport 上
int udpgrm_reuseport_prog(struct sk_reuseport_md *md)
{
    uint64_t socket_identifier = xxxx;
    bpf_sk_select_reuseport(md, &sockhash, &socket_identifier, 0);
    return SK_PASS;
}
为了选择特定的 socket，eBPF 程序会调用 bpf_sk_select_reuseport ，并通过一个指向"socket 映射"（ SOCKHASH 、 SOCKMAP 或太老已基本淘汰的 SOCKARRAY ）的引用，以及 key 或者 index 来工作。 比如，SOCKHASH 的一种可能的声明如下所示：

struct {
    __uint(type, BPF_MAP_TYPE_SOCKHASH);
    __uint(max_entries, MAX_SOCKETS);
    __uint(key_size, sizeof(uint64_t));
    __uint(value_size, sizeof(uint64_t));
} sockhash SEC(".maps");
这个 SOCKHASH 是一个 eBPF 里的哈希映射，它保存了对 socket 的引用，尽管其值的大小看起来像一个 8 字节的标量。在我们的例子中，它由一个 uint64_t 类型的 key 来索引。这非常简洁，我们可以进行简单的数字到 socket 的映射。

然而，有一个问题： SOCKHASH 必须在 userspace（或单独的控制平面）进行填充和维护，而不是在 eBPF 程序内（你需要管理更多复杂的状态，eBPF 可以使用的内存是受限的）。SOCKHASH 保持此 socket 映射的准确性并与服务器进程状态同步极其困难——尤其是在重启、崩溃或扩展事件等动态情况下。udpgrm 的重点就是处理这些事情，这样服务器进程就无需再处理了。

5. 银色子弹——udpgrm
前置知识
让我们来看看 udpgrm 是如何实现 UDP 流的优雅重启的。为了论述这套机制，我们需要先定义一些术语：一个 socket generation 是指在一个 reuseport 组中，隶属于同一个应用实例的一组套接字：

┌───────────────────────────────────────────────────┐
│ reuseport group 192.0.2.0:443                     │
│  ┌─────────────────────────────────────────────┐  │
│  │ socket generation 0                         │  │
│  │  ┌───────────┐ ┌───────────┐ ┌───────────┐  │  │
│  │  │ socket #1 │ │ socket #2 │ │ socket #3 │  │  │
│  │  └───────────┘ └───────────┘ └───────────┘  │  │
│  └─────────────────────────────────────────────┘  │
│  ┌─────────────────────────────────────────────┐  │
│  │ socket generation 1                         │  │
│  │  ┌───────────┐ ┌───────────┐ ┌───────────┐  │  │
│  │  │ socket #4 │ │ socket #5 │ │ socket #6 │  │  │
│  │  └───────────┘ └───────────┘ └───────────┘  │  │
│  └─────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────┘
当一个服务器进程需要重启时，新版本的进程会为其 socket 创建一个新的 socket generation。旧版本的进程则继续与新版本并行运行，使用来自前一个 socket generation 的 socket。

reuseport 的 eBPF 路由机制可以归结为两个问题：

对于新的 flow，我们应该从隶属于当前活动服务器实例的那个 socket generation 中选择一个 socket。 对于已经建立的 flow，我们应该选择合适的 socket——这个 socket 可能来自一个较旧的 socket generation——以保持会话的连续性（sticky）。这些旧的流最终会逐渐处理完毕并消失，从而允许旧的服务器实例关闭。 听起来很简单，对吧？

并非如此，细节还有很多东西需要推敲。让我们一步步来分析。

路由新的 flow 相对容易。udpgrm 仅仅维护一个指向应当处理新连接的 socket generation 的引用。我们称这个引用为 working generation。每当一个新流到达时，eBPF 程序会查询这个 working generation 指针，并从该 generation 中选择一个 socket。

┌──────────────────────────────────────────────┐
│ reuseport group 192.0.2.0:443                │
│   ...                                        │
│   Working generation ────┐                   │
│                          V                   │
│           ┌───────────────────────────────┐  │
│           │ socket generation 1           │  │
│           │  ┌───────────┐ ┌──────────┐   │  │
│           │  │ socket #4 │ │ ...      │   │  │
│           │  └───────────┘ └──────────┘   │  │
│           └───────────────────────────────┘  │
│   ...                                        │
└──────────────────────────────────────────────┘
为了让这套机制生效，我们首先需要能够区分隶属于新连接的数据包和隶属于旧连接的数据包。这一点非常棘手，并且高度依赖于具体的 UDP 协议。例如，QUIC 有一个初始包 (initial packet) 的概念，类似于 TCP 的 SYN 包，但其他协议可能没有。

这里需要一定的灵活性，udpgrm 将此设计为可配置项。每个 reuseport 组都可以设置一个特定的流解析器 (flow dissector)。

流解析器有两个任务：

它区分新数据包和隶属于已建立旧流的数据包。
对于已识别的 flow，它告诉 udpgrm 这个流具体属于哪个 socket。
这些概念紧密相关，并取决于具体的服务器实现。不同的 UDP 协议对"流"的定义也不同。例如，一个简单的 UDP 服务器可能会使用典型的五元组来定义流，而 QUIC 则使用其头部中的"connection id"字段，以便在发生 NAT rebinding 时依然能够识别连接。

udpgrm 默认支持三种流解析器，并且具有高度可配置性，以支持任何 UDP 协议。稍后会详细介绍。

既然我们已经介绍了理论，现在就可以进入正题了：欢迎 udpgrm（全称是 UDP Graceful Restart Marshal）! udpgrm 是一个有状态的守护进程，它处理了 UDP 优雅重启过程中的所有复杂性。它会安装合适的 eBPF REUSEPORT 程序，维护流状态，在重启期间与服务器进程通信，并报告一些数据以便于调试。

我们可以从两个视角来描述 udpgrm：系统管理员的视角和程序员的视角。

系统管理员眼里的 udpgrm
udpgrm 是一个有状态的守护进程，运行它：

$ sudo udpgrm --daemon
[ ] Loading BPF code
[ ] Pinning bpf programs to /sys/fs/bpf/udpgrm
[*] Tailing message ring buffer  map_id 936146
这会启动基础功能，打印初步的日志，并且应该作为一个专用的 systemd 服务来部署——在网络服务加载之后启动。然而，这还不足以完全使用 udpgrm。udpgrm 需要挂钩 (hook) 到 getsockopt、setsockopt、bind 和 sendmsg 这些系统调用上，而这些调用是与 cgroup 绑定的。要安装 udpgrm 的 hook point，你可以这样操作：

$ sudo udpgrm --install=/sys/fs/cgroup/system.slice
但一个更常见的模式是在当前 cgroup 内安装它：

sudo udpgrm --install --self
更好的做法是，将其作为 systemd "service"配置的一部分：

[Service]
ExecStart=/usr/bin/udpgrm --install --self
一旦 udpgrm 运行起来，管理员就可以使用命令行工具来列出 reuseport 组、socket 和各项数据，就像这样：

$ sudo udpgrm list
[ ] Retrieving BPF progs from /sys/fs/bpf/udpgrm
192.0.2.0:4433
    netns 0x1  dissector bespoke  digest 0xdead
    socket generations:
        gen  3  0x17a0da  <=  app 0  gen 3
    metrics:
        rx_processed_total 13777528077
...
现在，当 udpgrm 守护进程已经运行，并且 cgroup hooks 也已设置好，我们就可以专注于服务器端的部分了。

从程序员的角度看 udpgrm
我们期望服务器程序能自己创建合适的 UDP socket。我们依赖 SO_REUSEPORT，以便每个服务器实例可以拥有一个 socket 或一组的 socket generation：

sd = socket.socket(AF_INET, SOCK_DGRAM, 0)
sd.setsockopt(SOL_SOCKET, SO_REUSEPORT, 1)
sd.bind(("192.0.2.1", 5201))
创建一个 socket fd 后，我们就可以开始与 udpgrm 的交互了。服务器通过 setsockopt 调用与 udpgrm 守护进程通信。此外，udpgrm 提供了 eBPF 的 setsockopt 和 getsockopt hooks，并劫持了特定的 sys calls。在内核里面做这些并不容易，不过它的成果的确值得这些辛苦。一个典型的 socket 新增到 socket generation 的流程类似下面这样:

try:
    work_gen = sd.getsockopt(IPPROTO_UDP, UDP_GRM_WORKING_GEN)
except OSError:
    raise OSError('Is udpgrm daemon loaded? Try "udpgrm --self --install"')

sd.setsockopt(IPPROTO_UDP, UDP_GRM_SOCKET_GEN, work_gen + 1)
for i in range(10):
    v = sd.getsockopt(IPPROTO_UDP, UDP_GRM_SOCKET_GEN, 8);
    sk_gen, sk_idx = struct.unpack('II', v)
    if sk_idx != 0xffffffff:
        break
    time.sleep(0.01 * (2 ** i))
else:
    raise OSError("Communicating with udpgrm daemon failed.")

sd.setsockopt(IPPROTO_UDP, UDP_GRM_WORKING_GEN, work_gen + 1)
你可以看到这里可以分成三部分：

首先，我们获取当前的 working generation id，并以此来检查 udpgrm 是否存在。不过对于测试环境，udpgrm 不存在也是 OK 的。
然后，我们将这个 socket 注册到一个任意的 socket generation 中。我们选择 work_gen + 1 作为其编号，并验证注册是否成功。
最后，我们更新 working generation 指针，使其指向新的 generation。
就这么简单。除了这些代码外，udpgrm 守护进程还加载了 REUSEPORT eBPF 程序到内核，并且添加了定制数据结构（和上面的例子一样），追踪若干状态和数据，并在一个 SOCKHASH 中管理着这些 socket。

使用 udpgrm_activate.py 创建高级 socket
在实践中，我们经常需要将 socket 绑定到像 :443 这样的 Well-Known Port，这需要 CAP_NET_BIND_SERVICE 这样的提升权限。通常来说，在服务器自身外部配置监听 socket 会比较好。一种典型的模式是使用 socket 激活 (socket activation) 来传递监听 socket。

遗憾的是，systemd 无法为每个服务器实例创建一组新的 UDP SO_REUSEPORT socket，因为 systemd 不能支持 socket 新建或者更新。为了克服这个限制，udpgrm 提供了一个名为 udpgrm_activate.py 的脚本，可以这样使用：

[Service]
Type=notify                 # 启用对 fd　storage 的访问
NotifyAccess=all            # 允许从 ExecStartPre 访问 fd 存储
FileDescriptorStoreMax=128  # 必须设置存储 socket 的上限，eBPF 限制了 max_entries

ExecStartPre=/usr/local/bin/udpgrm_activate.py test-port 0.0.0.0:5201
这里，udpgrm_activate.py 会绑定到 0.0.0.0:5201，并将创建的 socket 以 test-port 的名称存储在 systemd 的 FD Storage 中。如果你用示例服务器 echoserver.py 来测试，它将继承这个 socket，并接收到相应的 FD_LISTEN 环境变量，遵循典型的 systemd socket 激活模式。

Systemd 服务生命周期问题
Systemd 通常无法处理多个服务器实例同时运行的情况。它倾向于快速杀死旧的实例。它支持的是"至多一个"服务器实例模型，而不是我们想要的"至少一个"模型。为了解决这个问题，udpgrm 提供了一个诱饵 (decoy) 脚本，当 systemd 要求它退出时，它会退出，而实际的旧服务器实例则可以在后台继续保持活动状态。

[Service]
...
ExecStart=/usr/local/bin/mmdecoy examples/echoserver.py

Restart=always             # 如果 pid 死亡，则重启它
KillMode=process           # 仅杀死诱饵进程，停止后保留子进程
KillSignal=SIGTERM         # 明确指定信号
至此，我们展示了一个启用了 udpgrm 的服务器的完整模板，它包含了所有三个要素：用于 cgroup 挂钩的 udpgrm --install --self，用于 socket 创建的 udpgrm_activate.py，以及用于欺骗 systemd 服务生命周期检查的 mmdecoy。

[Service]
Type=notify                 # 启用对 fd 存储的访问
NotifyAccess=all            # 允许从 ExecStartPre 访问 fd 存储
FileDescriptorStoreMax=128  # 必须设置存储 socket 的上限

ExecStartPre=/usr/local/bin/udpgrm --install --self
ExecStartPre=/usr/local/bin/udpgrm_activate.py --no-register test-port 0.0.0.0:5201
ExecStart=/usr/local/bin/mmdecoy PWD/examples/echoserver.py

Restart=always             # 如果 pid 死亡，则重启它
KillMode=process           # 仅杀死诱饵进程，停止后保留子进程
KillSignal=SIGTERM         # 明确指定信号
多重解析器模式
我们已经讨论了 udpgrm 守护进程、udpgrm 的 setsockopt API 以及 systemd 集成，但我们还没有涉及处理旧流的路由逻辑细节。为了处理任意协议，udpgrm 默认支持的三种解析器模式：

DISSECTOR_FLOW: udpgrm 维护一个流表，该表通过一个由典型四元组计算出的流哈希值来索引。它为每个流存储一个目标 socket 标识符。流表的大小是固定的，所以此模式支持的并发 flow 数量有限。为了将一个流标记为"已确认"，udpgrm 会 hook 到 sendmsg 这个 syscall，并仅在有消息发送时才将该 flow 保存到表中。
DISSECTOR_CBPF: 一种基于 cookie 的模型，其中目标 socket 标识符——称为 udpgrm cookie——被编码在每个入站 UDP 数据包中。例如，在 QUIC 中，这个标识符可以作为连接 ID 的一部分来存储。其解析逻辑以 cBPF 代码表示。这个模型不需要在 udpgrm 中维护流表，但集成起来更困难，因为它需要协议和服务器的支持。
DISSECTOR_NOOP: 一种完全没有状态追踪的空操作模式。它对于像 DNS 这样的传统 UDP 服务很有用，在这些服务中，我们希望在升级期间保证 0 丢包。
最后，udpgrm 提供了一个更高级的解析器模板，名为 DISSECTOR_BESPOKE（定制化解析器）。目前，它包含一个 QUIC 解析器，可以解码 QUIC 的 TLS SNI，并将特定的 TLS 主机名定向到特定的 socket generation。

如果想更多细节，可以查阅 udpgrm 的 README 。简而言之：FLOW 解析器是最简单的，适用于旧协议。CBPF 解析器适用于当协议允许存储自定义连接 ID (cookie) 时的实验性开发——我们用它开发了 Cloudflare 的 QUIC 连接 ID 方案（命名为 DCID）——但它速度很慢，因为它在 eBPF 内部解释执行 cBPF（没错，就是这么夸张。bpf_loop 就行，见此）。NOOP 很有用，但仅限于非常特定的 niche 服务器。真正的魔法在于 BESPOKE 类型，用户可以在其中创建任意的、快速且强大的解析器逻辑，利用二次开发来做到更强的灵活性。

快速实验结论
笔者读完原文 blog 后，把玩过了一轮原 github repo 中 DEMO。这次写完之后，基于上述的介绍，又制作了一些简单的实验。这里就不贴出代码了，基本都是由 udpgrm 的 example/ 下的代码修改而来。贴点简单的结论：

相对于无优雅重启状态下，这里的包延迟几乎无感，eBPF 的开销非常小。P95 延迟和中位数延迟都只是 3~5%的劣化。对比的是纯 UDP 服务器，未做任何优化，所以这个结果还是很能让人满意的，至少在生产中不会使用这样裸的 Server 吧。
吞吐上略微有一些优化（4 socketudpgrm 对比单 socketUDP server），udpgrm 能够支持更多的 socket 并发。
内核直接将包分发到不同 socket，这里能够减少一层拷贝开销。不过我测试下来数据有些波动，这里还需要再整理一下。
DISSECTOR_BESPOKE 还没有测试，不过看上去非常有趣。Cloudflare 做这个东西还是应该配合更多的 QUIC 的流量。
6.总结
QUIC 和其他基于 UDP 协议的普及，意味着优雅地重启 UDP 服务器正成为一个日益重要的问题。据我们所知，一个可重用的、可配置且易于使用的解决方案尚不存在。

udpgrm 项目汇集了几个有趣的想法：一个使用 setsockopt() 的简洁 API，精妙的 socket"窃取"逻辑，强大且富有表现力的可配置 flow 解析器，以及与 systemd 的集成。

udpgrm 非常易用。它隐藏了大量的复杂性，但最值得称赞的是，它解决了一个真正棘手的问题。归根结底，我们现在遇到的核心问题是，Linux 的 socket API 已经跟不上现代 UDP 的需求，我们只能缝缝补补用这些补丁来曲线救国。

从理想的角度来说，我认为这些功能中的大部分真的应该成为 systemd 的一个特性。这包括支持多服务器实例模型，UDP SO_REUSEPORT socket 的创建，加载 REUSEPORT_EBPF 程序，以及管理 working generation 指针。

我们希望 udpgrm 可以为这些长期的改进创造讨论的空间和术语基础。