//! Internationalization: English / 简体中文 / 繁體中文.
//!
//! The English source string is the lookup key; untranslated strings fall
//! back to English automatically. Tables are sorted for binary search.
//!
//! Author: Passwd Word

use serde::{Deserialize, Serialize};

/// UI language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Language {
    /// 简体中文 (default).
    #[default]
    ZhCn,
    /// 繁體中文.
    ZhTw,
    /// English.
    En,
}

impl Language {
    /// All languages in picker order.
    pub fn all() -> [Language; 3] {
        [Language::ZhCn, Language::ZhTw, Language::En]
    }

    /// Display name (always in its own language).
    pub fn name(&self) -> &'static str {
        match self {
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
            Language::En => "English",
        }
    }

    /// Stable settings key.
    pub fn code(&self) -> &'static str {
        match self {
            Language::ZhCn => "zh-CN",
            Language::ZhTw => "zh-TW",
            Language::En => "en",
        }
    }

    /// Parse a settings value; unknown values fall back to Simplified Chinese.
    pub fn from_code(s: &str) -> Self {
        match s {
            "zh-TW" => Language::ZhTw,
            "en" => Language::En,
            _ => Language::ZhCn,
        }
    }

    /// Translate an English source string. Falls back to the source itself.
    pub fn tr<'a>(&self, key: &'a str) -> &'a str {
        let table: &[(&str, &str)] = match self {
            Language::En => return key,
            Language::ZhCn => ZH_CN,
            Language::ZhTw => ZH_TW,
        };
        match table.binary_search_by(|&(k, _)| k.cmp(key)) {
            Ok(i) => table[i].1,
            Err(_) => key,
        }
    }

    /// Translate a template with `{name}` placeholders.
    pub fn tr_fmt(&self, key: &'static str, args: &[(&str, &str)]) -> String {
        let mut s = self.tr(key).to_string();
        for (name, value) in args {
            s = s.replace(&format!("{{{name}}}"), value);
        }
        s
    }
}

/// Simplified Chinese table (sorted by key).
static ZH_CN: &[(&str, &str)] = &[
    ("(empty)", "（空）"),
    ("/path/to/local/file", "/本地文件路径"),
    ("/path/to/synced/folder", "/同步文件夹路径"),
    ("0 = off, empty = 30", "0=关闭，空=30"),
    (
        "A local-first SSH workstation. Your data stays on your machine.",
        "本地优先的 SSH 工作站，数据只留在你的机器上。",
    ),
    ("About", "关于"),
    ("Action", "操作"),
    ("Actions", "操作"),
    ("Add host", "添加主机"),
    (
        "Add your first connection with the button below.",
        "通过下方按钮添加首个连接。",
    ),
    ("Address", "地址"),
    ("Agent", "代理"),
    ("All hosts", "全部主机"),
    ("Allow dangerous commands", "允许危险命令"),
    (
        "Any folder replicated by Syncthing / Nextcloud / rsync works.",
        "任何由 Syncthing / Nextcloud / rsync 同步的文件夹都可以。",
    ),
    ("Appearance", "外观"),
    ("Auth:", "认证方式："),
    ("Auto refresh", "自动刷新"),
    (
        "Bad bind address (use 127.0.0.1:8080)",
        "监听地址无效（示例 127.0.0.1:8080）",
    ),
    ("Bad host config: {e}", "主机配置错误：{e}"),
    ("Basic info", "基本信息"),
    ("Batch run", "批量执行"),
    ("Batch run finished", "批量执行完成"),
    ("Batch run started", "批量执行已开始"),
    ("Built-in editor", "内置编辑器"),
    ("CPU", "CPU"),
    ("CPU %", "CPU %"),
    ("Cancel", "取消"),
    ("Cannot create vault: {e}", "密码库创建失败：{e}"),
    ("Cannot save vault: {e}", "密码库保存失败：{e}"),
    ("Change master password", "修改主密码"),
    ("Changing password…", "正在修改密码…"),
    ("Check for updates", "检查更新"),
    ("Clear output", "清空输出"),
    ("Click to capture the keyboard", "点击捕获键盘输入"),
    (
        "Click “＋ New” to add your first server.",
        "点击「＋ 新建」添加你的第一台服务器。",
    ),
    ("Close", "关闭"),
    ("Close desktop", "关闭桌面"),
    ("Close tab", "关闭标签页"),
    ("Cloud repository", "云端仓库"),
    ("Collapse", "收起"),
    ("Collapse all", "全部折叠"),
    ("Collecting metrics…", "正在采集指标…"),
    ("Color", "颜色"),
    ("Coming soon in this build", "本版本即将上线"),
    ("Command:", "命令："),
    ("Confirm", "确认"),
    ("Confirm new:", "确认新密码："),
    ("Confirm password", "确认密码"),
    ("Connect", "连接"),
    ("Connect all", "并发"),
    (
        "Connect the host first (Terminal tab)",
        "请先连接主机（终端页）",
    ),
    ("Connect timeout (sec):", "连接超时（秒）:"),
    ("Connected to {name}", "已连接到 {name}"),
    ("Connecting to {title}…", "正在连接到 {title}…"),
    ("Connecting…", "连接中…"),
    ("Connection", "连接"),
    ("Connection OK ({ms} ms)", "连接成功（{ms} 毫秒）"),
    ("Connection closed.", "连接已关闭。"),
    ("Connection failed: {error}", "连接失败：{error}"),
    ("Copy", "复制"),
    ("Copy path", "复制路径"),
    ("Copy selection", "复制所选内容"),
    ("Cores", "核心"),
    ("Could not find the Downloads folder", "找不到下载文件夹"),
    ("Create", "创建"),
    (
        "Create a master password. It encrypts all stored credentials.",
        "设置一个主密码，用于加密保存所有凭据。",
    ),
    ("Create directory", "创建目录"),
    ("Create group", "创建分组"),
    ("Create vault", "创建密码库"),
    ("Credential", "凭据"),
    ("Credential deleted", "凭据已删除"),
    ("Credentials", "凭据"),
    (
        "Credentials never leave this device unencrypted.\nThere is no password recovery — don't forget it.",
        "凭据绝不会以未加密形式离开这台设备。\n主密码无法找回——千万别忘了。",
    ),
    ("Current password is wrong", "当前密码错误"),
    ("Current password:", "当前密码："),
    ("Current version: {current}", "当前版本：{current}"),
    ("Default path:", "默认路径:"),
    ("Delete", "删除"),
    ("Delete credential", "删除凭据"),
    (
        "Delete credential {name}? Hosts using it will no longer be able to sign in.",
        "删除凭据 {name}？使用它的主机将无法再登录。",
    ),
    ("Delete group", "删除分组"),
    ("Delete group “{name}”?", "删除分组“{name}”？"),
    ("Delete host", "删除主机"),
    ("Delete host “{name}”?", "删除主机“{name}”吗？"),
    ("Delete snippet", "删除代码片段"),
    ("Delete snippet “{title}”?", "删除代码片段“{title}”吗？"),
    ("Delete {n} hosts?", "删除 {n} 台主机？"),
    ("Delete {path}?", "删除 {path}？"),
    ("Delete “{path}”?", "删除“{path}”吗？"),
    ("Description:", "描述："),
    ("Desktop session is gone", "桌面会话已关闭"),
    ("Disconnect", "断开连接"),
    ("Disk", "磁盘"),
    ("Disk /", "磁盘 /"),
    ("Disk usage", "磁盘占用"),
    ("Download", "下载"),
    ("Download dir:", "下载目录："),
    ("Download queued", "下载已加入队列"),
    ("Downloading to {path}", "正在下载到 {path}"),
    ("Duplicate", "复制"),
    ("Edit", "编辑"),
    ("Edit host", "编辑主机"),
    ("Edit repository password", "编辑仓库密码"),
    ("Edit snippet", "编辑代码片段"),
    ("Enter a command first", "请先输入命令"),
    (
        "Enter a password or private key, or pick an existing credential.",
        "请输入密码或私钥，或选择一个已有凭据。",
    ),
    (
        "Enter your master password to unlock.",
        "输入主密码以解锁。",
    ),
    (
        "Env vars (KEY=value per line):",
        "环境变量（每行一个 KEY=value）：",
    ),
    ("Error: {err}", "错误：{err}"),
    ("Expand", "展开"),
    ("Export", "导出"),
    ("Export failed: {e}", "导出失败：{e}"),
    ("Export hosts…", "导出主机…"),
    ("Exported to {path}", "已导出到 {path}"),
    ("Favorites", "收藏"),
    ("File", "文件"),
    ("File operation", "文件操作"),
    ("File workspace", "文件工作区"),
    ("Files", "文件"),
    ("Files (SFTP)", "文件 (SFTP)"),
    ("Files panel", "文件面板"),
    ("Fingerprint (OpenSSH):", "指纹 (OpenSSH)："),
    ("Folder:", "文件夹："),
    ("Forward stopped", "转发已停止"),
    ("Forwarding {local}", "正在转发 {local}"),
    (
        "Forwards live as long as their SSH session. Start one from a connected session.",
        "转发随其 SSH 会话存续。请从已连接的会话发起。",
    ),
    ("Free", "空闲"),
    ("Frequent", "常用"),
    ("Go", "前往"),
    ("Group created", "分组已创建"),
    ("Group deleted", "分组已删除"),
    ("Group:", "分组："),
    ("Groups", "分组"),
    (
        "HTTP CONNECT and SOCKS5 are supported.",
        "支持 HTTP CONNECT 与 SOCKS5。",
    ),
    ("Help", "帮助"),
    ("Host deleted", "主机已删除"),
    ("Host is required.", "主机地址不能为空。"),
    ("Host not found", "找不到主机"),
    ("Host quick search", "主机快速搜索"),
    ("Host saved", "主机已保存"),
    ("Host:", "主机："),
    ("Host: {host}:{port}", "主机：{host}:{port}"),
    ("Hosts", "主机"),
    ("Hosts deleted", "主机已删除"),
    ("Hosts duplicated", "主机已复制"),
    ("Import", "导入"),
    ("Import failed: {e}", "导入失败：{e}"),
    ("Import hosts", "导入主机"),
    ("Import hosts…", "导入主机…"),
    ("Imported {count} host(s)", "已导入 {count} 台主机"),
    ("Info", "信息"),
    ("Init", "初始化"),
    ("Invalid name", "名称无效"),
    ("Jump chain too deep", "跳板链太深"),
    ("Jump host", "跳板机"),
    ("Jump host:", "跳板机："),
    ("Keepalive (sec):", "保活间隔（秒）:"),
    ("Keepalive command:", "保活命令:"),
    ("Keepalive period (sec):", "保活周期（秒）:"),
    ("Kernel", "内核"),
    ("Key", "密钥"),
    ("Key type: {key_type}", "密钥类型：{key_type}"),
    ("Keyboard shortcuts", "键盘快捷键"),
    ("Language", "语言"),
    ("Language:", "语言："),
    ("Last sync at {ts}", "上次同步时间：{ts}"),
    ("Last sync: {summary}", "上次同步：{summary}"),
    ("Latency", "延迟"),
    ("Linux hosts via /proc", "通过 /proc 采集 Linux 主机"),
    ("Load", "负载"),
    ("Loading…", "加载中…"),
    ("Local", "本地"),
    ("Local bind:", "本地绑定："),
    ("Local repository", "本地仓库"),
    ("MEM %", "内存 %"),
    ("Master password", "主密码"),
    ("Master password changed", "主密码已修改"),
    ("Memory", "内存"),
    ("Memory %", "内存 %"),
    ("Mode:", "模式："),
    ("Modified", "修改时间"),
    ("Monitor", "监控"),
    ("Monitor panel", "监控面板"),
    ("More", "更多"),
    ("Name", "名称"),
    ("Name is required.", "名称不能为空。"),
    ("Name:", "名称："),
    ("Network", "网络"),
    ("Network speed", "网速"),
    ("New", "新建"),
    ("New directory:", "新目录名："),
    ("New file", "新建文件"),
    ("New file…", "新建文件…"),
    ("New folder", "新建文件夹"),
    ("New folder…", "新建文件夹…"),
    ("New group", "新建分组"),
    (
        "New password must be at least 8 characters",
        "新密码至少 8 个字符",
    ),
    ("New password:", "新密码："),
    ("New passwords do not match", "两次输入的新密码不一致"),
    ("New port forward", "新建端口转发"),
    (
        "New port forward (local → remote)",
        "新建端口转发（本地 → 远程）",
    ),
    ("New snippet", "新建代码片段"),
    ("New tab (Ctrl+T)", "新标签页 (Ctrl+T)"),
    ("New terminal tab", "新建终端标签页"),
    ("New terminal tab    Ctrl+T", "新建终端标签页    Ctrl+T"),
    ("New version available: {latest}", "有新版本可用：{latest}"),
    ("New {p} connection", "新建{p}连接"),
    ("No active forwards", "没有活动转发"),
    ("No active terminal", "没有活动的终端"),
    ("No active terminal to split", "没有可分屏的活动终端"),
    ("No hosts yet.", "还没有主机。"),
    ("No open sessions", "没有打开的会话"),
    (
        "No snippets yet. Built-in examples are seeded on first run.",
        "还没有代码片段。首次运行时会预置内置示例。",
    ),
    ("No stored credentials.", "还没有保存的凭据。"),
    ("No terminal tabs", "没有终端标签页"),
    ("No transfers yet.", "还没有传输任务。"),
    (
        "No vault yet — set a master password so credentials can be stored encrypted.",
        "还没有密码库——设置主密码后，凭据才能加密保存。",
    ),
    ("No {p} connections yet", "暂无{p}连接"),
    ("Note", "备注"),
    ("Note:", "备注:"),
    ("Nothing to import", "没有可导入的内容"),
    ("OK", "确定"),
    ("Off", "关闭"),
    (
        "Only hosts, groups and snippets sync — and always end-to-end encrypted. Credentials never leave this device.",
        "只有主机、分组和片段会同步，且全程端到端加密。凭据绝不离开这台设备。",
    ),
    ("Open", "打开"),
    ("Open SFTP", "打开 SFTP"),
    ("Open in Files", "在文件页打开"),
    ("Open monitor", "打开监控"),
    ("Open terminal here", "在此打开终端"),
    ("Open with", "打开方式"),
    ("OpsDeck {current}", "OpsDeck {current}"),
    ("Other", "其他"),
    ("PID", "PID"),
    ("Parent directory", "上级目录"),
    ("Passphrase:", "密钥口令："),
    ("Password", "密码"),
    ("Password change failed: {e}", "主密码修改失败：{e}"),
    ("Password:", "密码："),
    ("Passwords do not match.", "两次输入的密码不一致。"),
    ("Paste", "粘贴"),
    ("Paste JSON here…", "在此粘贴 JSON…"),
    (
        "Paste the exported JSON below, then confirm.",
        "把导出的 JSON 粘贴到下方，然后确认。",
    ),
    ("Pause", "暂停"),
    ("Permissions", "权限"),
    ("Perms", "权限"),
    (
        "Pick a session and enter a valid remote host/port",
        "请选择会话并填写有效的远程主机/端口",
    ),
    ("Port forward failed: {error}", "端口转发失败：{error}"),
    ("Port forwarding…", "端口转发…"),
    ("Port must be a number.", "端口必须是数字。"),
    ("Port:", "端口："),
    ("Private key (PEM):", "私钥 (PEM)："),
    ("Processes", "进程"),
    ("Properties", "属性"),
    ("Protocol:", "协议:"),
    ("Proxy", "代理"),
    ("Proxy URL:", "代理地址:"),
    ("Quit", "退出"),
    ("Recent", "最近"),
    ("Reconnect", "重连"),
    ("Reconnect session", "重连会话"),
    ("Reconnected", "已重连"),
    ("Refresh", "刷新"),
    ("Refresh latency", "刷新延迟"),
    ("Reject", "拒绝"),
    ("Remote", "远端"),
    ("Remote host:", "远端主机："),
    ("Rename", "重命名"),
    ("Rename {from} →", "重命名 {from} →"),
    ("Rename…", "重命名…"),
    ("Retry", "重试"),
    ("Root directory", "根目录"),
    ("Run on active terminal", "在当前终端运行"),
    ("SFTP {op} done", "SFTP {op} 完成"),
    ("SFTP {op} failed: {e}", "SFTP {op} 失败：{e}"),
    ("SSH Workstation", "SSH 工作站"),
    ("SSH host key: {host}:{port}", "SSH 主机密钥：{host}:{port}"),
    ("Save", "保存"),
    ("Save and connect", "保存并连接"),
    ("Saving…", "保存中…"),
    ("Search hosts  (Ctrl+P)", "搜索主机  (Ctrl+P)"),
    ("Search snippets", "搜索代码片段"),
    ("Search terminal", "搜索终端"),
    ("Search terminal (Ctrl+F)", "搜索终端 (Ctrl+F)"),
    ("Select a file or folder first", "请先选中一个文件或文件夹"),
    ("Select a file to download", "请先选中一个文件再下载"),
    ("Select a file to edit", "请先选中一个文件再编辑"),
    (
        "Select a host on the left and press ▶ or double-click to connect.",
        "在左侧选一台主机，按 ▶ 或双击连接。",
    ),
    ("Select a host on the left first", "请先在左侧选择一台主机"),
    (
        "Select a host to browse its files.",
        "选择一台主机以浏览其文件。",
    ),
    ("Select a host to monitor.", "选择一台主机进行监控。"),
    ("Select at least one session", "请至少选择一个会话"),
    ("Server:", "服务器："),
    ("Session", "会话"),
    ("Session closed.", "会话已关闭。"),
    ("Session has no open shell", "会话没有打开的 shell"),
    ("Session:", "会话："),
    ("Sessions", "会话"),
    ("Set master password", "设置主密码"),
    (
        "Set up sync in Settings to use a cloud repository",
        "在设置中配置同步后才能使用云端仓库",
    ),
    ("Settings", "设置"),
    ("Shared folder", "共享文件夹"),
    ("Size", "大小"),
    ("Snippets", "代码片段"),
    ("Split left/right", "左右分屏"),
    (
        "Split left/right    Ctrl+Shift+E",
        "左右分屏    Ctrl+Shift+E",
    ),
    ("Split left/right (Ctrl+Shift+E)", "左右分屏 (Ctrl+Shift+E)"),
    ("Split up/down", "上下分屏"),
    ("Split up/down    Ctrl+Shift+O", "上下分屏    Ctrl+Shift+O"),
    ("Split up/down (Ctrl+Shift+O)", "上下分屏 (Ctrl+Shift+O)"),
    ("Start", "启动"),
    ("Startup cmd:", "启动命令："),
    ("Stop", "停止"),
    ("Sync", "同步"),
    ("Sync complete", "同步完成"),
    ("Sync error: {err}", "同步错误：{err}"),
    ("Sync failed: {e}", "同步失败：{e}"),
    (
        "Sync is off — pick a mode in Settings",
        "同步未开启——在设置中选择同步模式",
    ),
    ("Sync now", "立即同步"),
    ("Sync server", "同步服务器"),
    ("Syncing…", "同步中…"),
    ("System", "系统"),
    ("Tags (comma separated):", "标签（逗号分隔）："),
    ("Tags:", "标签："),
    ("Targets:", "目标："),
    ("Terminal", "终端"),
    ("Terminal font size", "终端字体大小"),
    ("Terminal type:", "终端类型:"),
    ("Test connection", "测试连接"),
    ("Testing connection…", "正在测试连接…"),
    (
        "The host must be connected. Metrics are polled every few seconds.",
        "主机必须已连接。指标每几秒轮询一次。",
    ),
    ("Theme", "主题"),
    ("Theme:", "主题："),
    (
        "This server is not in your known-hosts database:",
        "这台服务器不在你的已知主机库中：",
    ),
    ("Title and command are required", "标题和命令不能为空"),
    ("Title:", "标题："),
    ("Toggle favorite", "切换收藏"),
    ("Top processes", "占用最高的进程"),
    ("Total", "总共"),
    ("Transfer failed: {e}", "传输失败：{e}"),
    ("Transfer {task_id} done", "传输任务 {task_id} 完成"),
    ("Transfers", "传输任务"),
    ("Trust always", "始终信任"),
    ("Trust once", "仅信任这一次"),
    ("Tunnels", "隧道"),
    ("UI font size", "界面字体大小"),
    ("Ungrouped", "未分组"),
    ("Unknown action: {other}", "未知操作：{other}"),
    ("Unlock", "解锁"),
    ("Unlock vault", "解锁保险库"),
    ("Unreachable", "不可达"),
    ("Update available: {latest}", "有新版本可用：{latest}"),
    ("Update check failed: {err}", "更新检查失败：{err}"),
    ("Updates", "更新"),
    ("Upload", "上传"),
    ("Upload into {dir}", "上传到 {dir}"),
    ("Upload local file:", "上传本地文件："),
    ("Upload queued", "上传已加入队列"),
    ("Upload to cloud", "上云"),
    ("Uptime", "运行时间"),
    ("Use at least 8 characters.", "请至少使用 8 个字符。"),
    ("Used", "已用"),
    ("Username is required.", "用户名不能为空。"),
    ("Username:", "用户名："),
    (
        "Uses your local SSH agent (no stored secret).",
        "使用你本地的 SSH 代理（不保存任何密钥）。",
    ),
    ("Vault", "密码库"),
    ("Vault created", "密码库已创建"),
    ("Vault is locked", "保险库已锁定"),
    (
        "Vault is locked — unlock it to connect.",
        "密码库已锁定——解锁后才能连接。",
    ),
    (
        "Vault is locked — unlock it to save this host.",
        "密码库已锁定——解锁后才能保存这台主机。",
    ),
    ("Vault is locked.", "保险库已锁定。"),
    ("Vault is unlocked.", "保险库已解锁。"),
    ("Vault locked", "保险库已锁定"),
    ("View", "视图"),
    ("View all", "查看全部"),
    ("Wrong password.", "密码错误。"),
    ("You're up to date", "已是最新版本"),
    ("Zoom in", "放大"),
    ("Zoom out", "缩小"),
    ("canceled", "已取消"),
    ("default: ~/Downloads", "默认：~/Downloads"),
    ("done", "完成"),
    ("empty = default", "空=默认"),
    ("failed", "失败"),
    ("no open sessions", "没有打开的会话"),
    ("not connected", "未连接"),
    ("optional", "可选"),
    ("paused", "已暂停"),
    ("port:", "端口："),
    ("running", "进行中"),
    ("sync on", "同步已开启"),
    ("used by {n} host(s)", "被 {n} 台主机使用"),
    ("{name}: connection closed", "{name}：连接已关闭"),
    ("{n} selected", "已选 {n} 项"),
    ("{n} session(s)", "{n} 个会话"),
    (
        "{user}@{host}:{port}\nDouble-click to connect",
        "{user}@{host}:{port}\n双击连接",
    ),
    ("— new —", "— 新建 —"),
    ("— none —", "— 无 —"),
    ("— select —", "— 请选择 —"),
    ("— {msg} [press ↻ to reconnect]", "— {msg}  [按 ↻ 重连]"),
    ("⇄ {n} forward(s)", "⇄ {n} 个转发"),
    ("▶ Run on selected", "▶ 在选中项上运行"),
    ("★ Favorite", "★ 收藏"),
    ("★ Favorites", "★ 收藏"),
    (
        "⚠ WARNING: the host key CHANGED. This may indicate a man-in-the-middle attack.",
        "⚠ 警告：主机密钥已变更。可能存在中间人攻击。",
    ),
    (
        "⚠ {n} host key approval(s) pending",
        "⚠ {n} 个主机密钥待确认",
    ),
    ("✏️ Edit", "✏️ 编辑"),
    ("⤒ Import", "⤒ 导入"),
    ("⤓ Export", "⤓ 导出"),
    ("⬆ Update available: {latest}", "⬆ 有可用更新：{latest}"),
    ("⬆ Upload", "⬆ 上传"),
    ("⬇ Download", "⬇ 下载"),
    ("＋ Add", "＋ 添加"),
    ("＋ Dir", "＋ 新建目录"),
    ("＋ Group", "＋分组"),
    ("＋ New", "＋ 新建"),
    ("＋ New tab", "＋ 新标签页"),
    ("🔒 Lock vault", "🔒 锁定密码库"),
    ("🗑 Delete", "🗑 删除"),
];

/// Traditional Chinese table (sorted by key).
static ZH_TW: &[(&str, &str)] = &[
    ("(empty)", "（空）"),
    ("/path/to/local/file", "/本機檔案路徑"),
    ("/path/to/synced/folder", "/同步資料夾路徑"),
    ("0 = off, empty = 30", "0=關閉，空=30"),
    (
        "A local-first SSH workstation. Your data stays on your machine.",
        "本地優先的 SSH 工作站，資料只留在你的機器上。",
    ),
    ("About", "關於"),
    ("Action", "操作"),
    ("Actions", "操作"),
    ("Add host", "新增主機"),
    (
        "Add your first connection with the button below.",
        "透過下方按鈕新增首個連線。",
    ),
    ("Address", "地址"),
    ("Agent", "代理"),
    ("All hosts", "全部主機"),
    ("Allow dangerous commands", "允許危險命令"),
    (
        "Any folder replicated by Syncthing / Nextcloud / rsync works.",
        "任何由 Syncthing / Nextcloud / rsync 同步的資料夾都可以。",
    ),
    ("Appearance", "外觀"),
    ("Auth:", "驗證方式："),
    ("Auto refresh", "自動重新整理"),
    (
        "Bad bind address (use 127.0.0.1:8080)",
        "監聽位址無效（範例 127.0.0.1:8080）",
    ),
    ("Bad host config: {e}", "主機設定錯誤：{e}"),
    ("Basic info", "基本資訊"),
    ("Batch run", "批次執行"),
    ("Batch run finished", "批次執行完成"),
    ("Batch run started", "批次執行已開始"),
    ("Built-in editor", "內置編輯器"),
    ("CPU", "CPU"),
    ("CPU %", "CPU %"),
    ("Cancel", "取消"),
    ("Cannot create vault: {e}", "密碼庫建立失敗：{e}"),
    ("Cannot save vault: {e}", "密碼庫儲存失敗：{e}"),
    ("Change master password", "變更主密碼"),
    ("Changing password…", "正在變更密碼…"),
    ("Check for updates", "檢查更新"),
    ("Clear output", "清除輸出"),
    ("Click to capture the keyboard", "點擊擷取鍵盤輸入"),
    (
        "Click “＋ New” to add your first server.",
        "點擊「＋ 新增」加入你的第一台伺服器。",
    ),
    ("Close", "關閉"),
    ("Close desktop", "關閉桌面"),
    ("Close tab", "關閉標籤頁"),
    ("Cloud repository", "雲端倉庫"),
    ("Collapse", "收起"),
    ("Collapse all", "全部摺疊"),
    ("Collecting metrics…", "正在採集指標…"),
    ("Color", "顏色"),
    ("Coming soon in this build", "本版本即將上線"),
    ("Command:", "命令："),
    ("Confirm", "確認"),
    ("Confirm new:", "確認新密碼："),
    ("Confirm password", "確認密碼"),
    ("Connect", "連線"),
    ("Connect all", "並發"),
    (
        "Connect the host first (Terminal tab)",
        "請先連接主機（終端頁）",
    ),
    ("Connect timeout (sec):", "連線逾時（秒）:"),
    ("Connected to {name}", "已連線到 {name}"),
    ("Connecting to {title}…", "正在連線到 {title}…"),
    ("Connecting…", "連線中…"),
    ("Connection", "連線"),
    ("Connection OK ({ms} ms)", "連線成功（{ms} 毫秒）"),
    ("Connection closed.", "連線已關閉。"),
    ("Connection failed: {error}", "連線失敗：{error}"),
    ("Copy", "複製"),
    ("Copy path", "複製路徑"),
    ("Copy selection", "複製所選內容"),
    ("Cores", "核心"),
    ("Could not find the Downloads folder", "找不到下載資料夾"),
    ("Create", "建立"),
    (
        "Create a master password. It encrypts all stored credentials.",
        "設定一個主密碼，用於加密保存所有憑證。",
    ),
    ("Create directory", "建立目錄"),
    ("Create group", "建立群組"),
    ("Create vault", "建立密碼庫"),
    ("Credential", "憑證"),
    ("Credential deleted", "憑據已刪除"),
    ("Credentials", "憑據"),
    (
        "Credentials never leave this device unencrypted.\nThere is no password recovery — don't forget it.",
        "憑證絕不會以未加密形式離開這台裝置。\n主密碼無法找回——千萬別忘了。",
    ),
    ("Current password is wrong", "目前密碼錯誤"),
    ("Current password:", "目前密碼："),
    ("Current version: {current}", "目前版本：{current}"),
    ("Default path:", "預設路徑:"),
    ("Delete", "刪除"),
    ("Delete credential", "刪除憑據"),
    (
        "Delete credential {name}? Hosts using it will no longer be able to sign in.",
        "刪除憑證 {name}？使用它的主機將無法再登入。",
    ),
    ("Delete group", "刪除分組"),
    ("Delete group “{name}”?", "刪除分組「{name}」？"),
    ("Delete host", "刪除主機"),
    ("Delete host “{name}”?", "確定刪除主機「{name}」嗎？"),
    ("Delete snippet", "刪除程式碼片段"),
    ("Delete snippet “{title}”?", "刪除片段「{title}」？"),
    ("Delete {n} hosts?", "刪除 {n} 台主機？"),
    ("Delete {path}?", "刪除 {path}？"),
    ("Delete “{path}”?", "確定刪除「{path}」嗎？"),
    ("Description:", "描述："),
    ("Desktop session is gone", "桌面會話已關閉"),
    ("Disconnect", "中斷連線"),
    ("Disk", "磁碟"),
    ("Disk /", "磁碟 /"),
    ("Disk usage", "磁碟使用"),
    ("Download", "下載"),
    ("Download dir:", "下載目錄："),
    ("Download queued", "下載已加入佇列"),
    ("Downloading to {path}", "正在下載到 {path}"),
    ("Duplicate", "複製"),
    ("Edit", "編輯"),
    ("Edit host", "編輯主機"),
    ("Edit repository password", "編輯倉庫密碼"),
    ("Edit snippet", "編輯程式碼片段"),
    ("Enter a command first", "請先輸入命令"),
    (
        "Enter a password or private key, or pick an existing credential.",
        "請輸入密碼或私鑰，或選擇一個已有憑證。",
    ),
    (
        "Enter your master password to unlock.",
        "輸入主密碼以解鎖。",
    ),
    (
        "Env vars (KEY=value per line):",
        "環境變數（每行一個 KEY=value）：",
    ),
    ("Error: {err}", "錯誤：{err}"),
    ("Expand", "展開"),
    ("Export", "匯出"),
    ("Export failed: {e}", "匯出失敗：{e}"),
    ("Export hosts…", "匯出主機…"),
    ("Exported to {path}", "已匯出到 {path}"),
    ("Favorites", "收藏"),
    ("File", "檔案"),
    ("File operation", "檔案操作"),
    ("File workspace", "檔案工作區"),
    ("Files", "檔案"),
    ("Files (SFTP)", "檔案 (SFTP)"),
    ("Files panel", "文件面板"),
    ("Fingerprint (OpenSSH):", "指紋 (OpenSSH)："),
    ("Folder:", "資料夾："),
    ("Forward stopped", "轉發已停止"),
    ("Forwarding {local}", "正在轉發 {local}"),
    (
        "Forwards live as long as their SSH session. Start one from a connected session.",
        "轉發隨其 SSH 工作階段存續。請從已連接的工作階段發起。",
    ),
    ("Free", "閒置"),
    ("Frequent", "常用"),
    ("Go", "前往"),
    ("Group created", "分組已建立"),
    ("Group deleted", "分組已刪除"),
    ("Group:", "群組："),
    ("Groups", "分組"),
    (
        "HTTP CONNECT and SOCKS5 are supported.",
        "支援 HTTP CONNECT 與 SOCKS5。",
    ),
    ("Help", "說明"),
    ("Host deleted", "主機已刪除"),
    ("Host is required.", "主機位址不能為空。"),
    ("Host not found", "找不到主機"),
    ("Host quick search", "主機快速搜尋"),
    ("Host saved", "主機已儲存"),
    ("Host:", "主機："),
    ("Host: {host}:{port}", "主機：{host}:{port}"),
    ("Hosts", "主機"),
    ("Hosts deleted", "主機已刪除"),
    ("Hosts duplicated", "主機已複製"),
    ("Import", "匯入"),
    ("Import failed: {e}", "匯入失敗：{e}"),
    ("Import hosts", "匯入主機"),
    ("Import hosts…", "匯入主機…"),
    ("Imported {count} host(s)", "已匯入 {count} 台主機"),
    ("Info", "資訊"),
    ("Init", "初始化"),
    ("Invalid name", "名稱無效"),
    ("Jump chain too deep", "跳板鏈太深"),
    ("Jump host", "跳板機"),
    ("Jump host:", "跳板機："),
    ("Keepalive (sec):", "保活間隔（秒）:"),
    ("Keepalive command:", "保活命令:"),
    ("Keepalive period (sec):", "保活週期（秒）:"),
    ("Kernel", "內核"),
    ("Key", "金鑰"),
    ("Key type: {key_type}", "金鑰類型：{key_type}"),
    ("Keyboard shortcuts", "鍵盤快速鍵"),
    ("Language", "語言"),
    ("Language:", "語言："),
    ("Last sync at {ts}", "上次同步時間：{ts}"),
    ("Last sync: {summary}", "上次同步：{summary}"),
    ("Latency", "延遲"),
    ("Linux hosts via /proc", "透過 /proc 採集 Linux 主機"),
    ("Load", "負載"),
    ("Loading…", "載入中…"),
    ("Local", "本機"),
    ("Local bind:", "本機綁定："),
    ("Local repository", "本機倉庫"),
    ("MEM %", "記憶體 %"),
    ("Master password", "主密碼"),
    ("Master password changed", "主密碼已變更"),
    ("Memory", "記憶體"),
    ("Memory %", "記憶體 %"),
    ("Mode:", "模式："),
    ("Modified", "修改時間"),
    ("Monitor", "監控"),
    ("Monitor panel", "監控面板"),
    ("More", "更多"),
    ("Name", "名稱"),
    ("Name is required.", "名稱不能為空。"),
    ("Name:", "名稱："),
    ("Network", "網路"),
    ("Network speed", "網速"),
    ("New", "新增"),
    ("New directory:", "新目錄名稱："),
    ("New file", "新增檔案"),
    ("New file…", "新增檔案…"),
    ("New folder", "新增資料夾"),
    ("New folder…", "新增資料夾…"),
    ("New group", "新建分組"),
    (
        "New password must be at least 8 characters",
        "新密碼至少 8 個字元",
    ),
    ("New password:", "新密碼："),
    ("New passwords do not match", "兩次輸入的新密碼不一致"),
    ("New port forward", "新增連接埠轉送"),
    (
        "New port forward (local → remote)",
        "新建連接埠轉發（本地 → 遠端）",
    ),
    ("New snippet", "新增程式碼片段"),
    ("New tab (Ctrl+T)", "新標籤頁 (Ctrl+T)"),
    ("New terminal tab", "新增終端標籤頁"),
    ("New terminal tab    Ctrl+T", "新增終端標籤頁    Ctrl+T"),
    ("New version available: {latest}", "有新版本可用：{latest}"),
    ("New {p} connection", "新建{p}連線"),
    ("No active forwards", "沒有活動轉送"),
    ("No active terminal", "沒有作用中的終端"),
    ("No active terminal to split", "沒有可分割的作用中終端"),
    ("No hosts yet.", "尚無主機。"),
    ("No open sessions", "沒有開啟的連線"),
    (
        "No snippets yet. Built-in examples are seeded on first run.",
        "還沒有程式碼片段。首次執行時會預置內置範例。",
    ),
    ("No stored credentials.", "還沒有儲存的憑據。"),
    ("No terminal tabs", "沒有終端標籤頁"),
    ("No transfers yet.", "尚無傳輸任務。"),
    (
        "No vault yet — set a master password so credentials can be stored encrypted.",
        "還沒有密碼庫——設定主密碼後，憑證才能加密保存。",
    ),
    ("No {p} connections yet", "暫無{p}連線"),
    ("Note", "備註"),
    ("Note:", "備註:"),
    ("Nothing to import", "沒有可匯入的內容"),
    ("OK", "確定"),
    ("Off", "關閉"),
    (
        "Only hosts, groups and snippets sync — and always end-to-end encrypted. Credentials never leave this device.",
        "只有主機、群組和片段會同步，且全程端到端加密。憑證絕不離開這台裝置。",
    ),
    ("Open", "開啟"),
    ("Open SFTP", "開啟 SFTP"),
    ("Open in Files", "在文件頁開啟"),
    ("Open monitor", "開啟監控"),
    ("Open terminal here", "在此開啟終端"),
    ("Open with", "開啟方式"),
    ("OpsDeck {current}", "OpsDeck {current}"),
    ("Other", "其他"),
    ("PID", "PID"),
    ("Parent directory", "上層目錄"),
    ("Passphrase:", "金鑰口令："),
    ("Password", "密碼"),
    ("Password change failed: {e}", "主密碼變更失敗：{e}"),
    ("Password:", "密碼："),
    ("Passwords do not match.", "兩次輸入的密碼不一致。"),
    ("Paste", "貼上"),
    ("Paste JSON here…", "在此貼上 JSON…"),
    (
        "Paste the exported JSON below, then confirm.",
        "把匯出的 JSON 貼到下方，然後確認。",
    ),
    ("Pause", "暫停"),
    ("Permissions", "權限"),
    ("Perms", "權限"),
    (
        "Pick a session and enter a valid remote host/port",
        "請選擇工作階段並填寫有效的遠端主機/連接埠",
    ),
    ("Port forward failed: {error}", "通訊埠轉發失敗：{error}"),
    ("Port forwarding…", "通訊埠轉發…"),
    ("Port must be a number.", "通訊埠必須是數字。"),
    ("Port:", "通訊埠："),
    ("Private key (PEM):", "私鑰 (PEM)："),
    ("Processes", "處理程序"),
    ("Properties", "屬性"),
    ("Protocol:", "協定:"),
    ("Proxy", "代理"),
    ("Proxy URL:", "代理位址:"),
    ("Quit", "離開"),
    ("Recent", "最近"),
    ("Reconnect", "重新連線"),
    ("Reconnect session", "重新連線"),
    ("Reconnected", "已重新連線"),
    ("Refresh", "重新整理"),
    ("Refresh latency", "重新整理延遲"),
    ("Reject", "拒絕"),
    ("Remote", "遠端"),
    ("Remote host:", "遠端主機："),
    ("Rename", "重新命名"),
    ("Rename {from} →", "重新命名 {from} →"),
    ("Rename…", "重新命名…"),
    ("Retry", "重試"),
    ("Root directory", "根目錄"),
    ("Run on active terminal", "在目前終端執行"),
    ("SFTP {op} done", "SFTP {op} 完成"),
    ("SFTP {op} failed: {e}", "SFTP {op} 失敗：{e}"),
    ("SSH Workstation", "SSH 工作站"),
    ("SSH host key: {host}:{port}", "SSH 主機金鑰：{host}:{port}"),
    ("Save", "儲存"),
    ("Save and connect", "儲存並連線"),
    ("Saving…", "儲存中…"),
    ("Search hosts  (Ctrl+P)", "搜尋主機  (Ctrl+P)"),
    ("Search snippets", "搜尋程式碼片段"),
    ("Search terminal", "搜尋終端"),
    ("Search terminal (Ctrl+F)", "搜尋終端 (Ctrl+F)"),
    ("Select a file or folder first", "請先選中一個檔案或資料夾"),
    ("Select a file to download", "請先選中一個檔案再下載"),
    ("Select a file to edit", "請先選中一個檔案再編輯"),
    (
        "Select a host on the left and press ▶ or double-click to connect.",
        "在左側選一台主機，按 ▶ 或雙擊連接。",
    ),
    ("Select a host on the left first", "請先在左側選擇一台主機"),
    (
        "Select a host to browse its files.",
        "選擇一台主機以瀏覽其檔案。",
    ),
    ("Select a host to monitor.", "選擇一台主機進行監控。"),
    ("Select at least one session", "請至少選擇一個連線"),
    ("Server:", "伺服器："),
    ("Session", "連線"),
    ("Session closed.", "連線已關閉。"),
    ("Session has no open shell", "連線沒有開啟的 shell"),
    ("Session:", "連線："),
    ("Sessions", "連線"),
    ("Set master password", "設定主密碼"),
    (
        "Set up sync in Settings to use a cloud repository",
        "在設定中配置同步後才能使用雲端倉庫",
    ),
    ("Settings", "設定"),
    ("Shared folder", "共用資料夾"),
    ("Size", "大小"),
    ("Snippets", "程式碼片段"),
    ("Split left/right", "左右分割"),
    (
        "Split left/right    Ctrl+Shift+E",
        "左右分屏    Ctrl+Shift+E",
    ),
    ("Split left/right (Ctrl+Shift+E)", "左右分割 (Ctrl+Shift+E)"),
    ("Split up/down", "上下分割"),
    ("Split up/down    Ctrl+Shift+O", "上下分割    Ctrl+Shift+O"),
    ("Split up/down (Ctrl+Shift+O)", "上下分割 (Ctrl+Shift+O)"),
    ("Start", "啟動"),
    ("Startup cmd:", "啟動命令："),
    ("Stop", "停止"),
    ("Sync", "同步"),
    ("Sync complete", "同步完成"),
    ("Sync error: {err}", "同步錯誤：{err}"),
    ("Sync failed: {e}", "同步失敗：{e}"),
    (
        "Sync is off — pick a mode in Settings",
        "同步未開啟——在設定中選擇同步模式",
    ),
    ("Sync now", "立即同步"),
    ("Sync server", "同步伺服器"),
    ("Syncing…", "同步中…"),
    ("System", "系統"),
    ("Tags (comma separated):", "標籤（逗號分隔）："),
    ("Tags:", "標籤："),
    ("Targets:", "目標："),
    ("Terminal", "終端"),
    ("Terminal font size", "終端字型大小"),
    ("Terminal type:", "終端機類型:"),
    ("Test connection", "測試連線"),
    ("Testing connection…", "正在測試連線…"),
    (
        "The host must be connected. Metrics are polled every few seconds.",
        "主機必須已連接。指標每幾秒輪詢一次。",
    ),
    ("Theme", "主題"),
    ("Theme:", "主題："),
    (
        "This server is not in your known-hosts database:",
        "這台伺服器不在你的已知主機庫中：",
    ),
    ("Title and command are required", "標題和命令不能為空"),
    ("Title:", "標題："),
    ("Toggle favorite", "切換收藏"),
    ("Top processes", "資源佔用最高的處理程序"),
    ("Total", "總共"),
    ("Transfer failed: {e}", "傳輸失敗：{e}"),
    ("Transfer {task_id} done", "傳輸任務 {task_id} 完成"),
    ("Transfers", "傳輸任務"),
    ("Trust always", "永久信任"),
    ("Trust once", "僅信任這一次"),
    ("Tunnels", "隧道"),
    ("UI font size", "介面字型大小"),
    ("Ungrouped", "未分組"),
    ("Unknown action: {other}", "未知操作：{other}"),
    ("Unlock", "解鎖"),
    ("Unlock vault", "解鎖保險庫"),
    ("Unreachable", "不可達"),
    ("Update available: {latest}", "有新版本可用：{latest}"),
    ("Update check failed: {err}", "更新檢查失敗：{err}"),
    ("Updates", "更新"),
    ("Upload", "上傳"),
    ("Upload into {dir}", "上傳到 {dir}"),
    ("Upload local file:", "上傳本機檔案："),
    ("Upload queued", "上傳已加入佇列"),
    ("Upload to cloud", "上雲"),
    ("Uptime", "運行時間"),
    ("Use at least 8 characters.", "請至少使用 8 個字元。"),
    ("Used", "已用"),
    ("Username is required.", "使用者名稱不能為空。"),
    ("Username:", "使用者名稱："),
    (
        "Uses your local SSH agent (no stored secret).",
        "使用你本地的 SSH 代理（不保存任何密鑰）。",
    ),
    ("Vault", "密碼庫"),
    ("Vault created", "密碼庫已建立"),
    ("Vault is locked", "保險庫已鎖定"),
    (
        "Vault is locked — unlock it to connect.",
        "密碼庫已鎖定——解鎖後才能連接。",
    ),
    (
        "Vault is locked — unlock it to save this host.",
        "密碼庫已鎖定——解鎖後才能保存這台主機。",
    ),
    ("Vault is locked.", "保險庫已鎖定。"),
    ("Vault is unlocked.", "保險庫已解鎖。"),
    ("Vault locked", "保險庫已鎖定"),
    ("View", "檢視"),
    ("View all", "查看全部"),
    ("Wrong password.", "密碼錯誤。"),
    ("You're up to date", "已是最新版本"),
    ("Zoom in", "放大"),
    ("Zoom out", "縮小"),
    ("canceled", "已取消"),
    ("default: ~/Downloads", "預設：~/Downloads"),
    ("done", "完成"),
    ("empty = default", "空=預設"),
    ("failed", "失敗"),
    ("no open sessions", "沒有開啟的連線"),
    ("not connected", "未連線"),
    ("optional", "選填"),
    ("paused", "已暫停"),
    ("port:", "通訊埠："),
    ("running", "進行中"),
    ("sync on", "同步已開啟"),
    ("used by {n} host(s)", "被 {n} 台主機使用"),
    ("{name}: connection closed", "{name}：連線已關閉"),
    ("{n} selected", "已選 {n} 項"),
    ("{n} session(s)", "{n} 個連線"),
    (
        "{user}@{host}:{port}\nDouble-click to connect",
        "{user}@{host}:{port}\n雙擊連接",
    ),
    ("— new —", "— 新增 —"),
    ("— none —", "— 無 —"),
    ("— select —", "— 請選擇 —"),
    ("— {msg} [press ↻ to reconnect]", "— {msg}  [按 ↻ 重新連線]"),
    ("⇄ {n} forward(s)", "⇄ {n} 個轉發"),
    ("▶ Run on selected", "▶ 在選取項目上執行"),
    ("★ Favorite", "★ 收藏"),
    ("★ Favorites", "★ 收藏"),
    (
        "⚠ WARNING: the host key CHANGED. This may indicate a man-in-the-middle attack.",
        "⚠ 警告：主機金鑰已變更。可能存在中間人攻擊。",
    ),
    (
        "⚠ {n} host key approval(s) pending",
        "⚠ {n} 個主機金鑰待確認",
    ),
    ("✏️ Edit", "✏️ 編輯"),
    ("⤒ Import", "⤒ 匯入"),
    ("⤓ Export", "⤓ 匯出"),
    ("⬆ Update available: {latest}", "⬆ 有可用更新：{latest}"),
    ("⬆ Upload", "⬆ 上傳"),
    ("⬇ Download", "⬇ 下載"),
    ("＋ Add", "＋ 新增"),
    ("＋ Dir", "＋ 新增目錄"),
    ("＋ Group", "＋群組"),
    ("＋ New", "＋ 新增"),
    ("＋ New tab", "＋ 新標籤頁"),
    ("🔒 Lock vault", "🔒 鎖定密碼庫"),
    ("🗑 Delete", "🗑 刪除"),
];

#[cfg(test)]
mod tests {
    use super::{ZH_CN, ZH_TW};

    fn sorted_unique(table: &[(&str, &str)]) {
        let mut prev: Option<&str> = None;
        for (k, _) in table {
            if let Some(p) = prev {
                assert!(p < *k, "table not sorted or duplicate: {p:?} vs {k:?}");
            }
            prev = Some(k);
        }
    }

    /// Regression guard: every literal key passed to `t()`/`tf()` anywhere
    /// in the ui/app sources must have a real translation in BOTH tables
    /// (a missing key silently falls back to English in the UI).
    #[test]
    fn all_source_keys_translated() {
        const ALLOW_SAME: &[&str] = &["CPU", "CPU %", "PID", "OpsDeck {current}"];
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        let mut stack = vec![root.join("src")];
        // The app crate lives next door; scan it too.
        stack.push(root.join("../app/src"));
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("read src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && path.file_name().is_some_and(|n| n != "i18n.rs")
                {
                    // (i18n.rs itself is excluded: this test's own source
                    // contains the search patterns as string literals.)
                    files.push(path);
                }
            }
        }
        let mut keys: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for file in &files {
            let text = std::fs::read_to_string(file).expect("read rs file");
            let bytes = text.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                // Find `.t(` or `.tf(` then the string literal.
                if bytes[i] == b'.' {
                    let rest = &text[i..];
                    let after = if rest.starts_with(".tf(") || rest.starts_with(".tr(") {
                        Some(i + 4)
                    } else if rest.starts_with(".t(") {
                        Some(i + 3)
                    } else {
                        None
                    };
                    if let Some(mut j) = after {
                        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                            j += 1;
                        }
                        if j < bytes.len() && bytes[j] == b'"' {
                            j += 1;
                            let mut key = String::new();
                            while j < bytes.len() && bytes[j] != b'"' {
                                if bytes[j] == b'\\' && j + 1 < bytes.len() {
                                    match bytes[j + 1] {
                                        b'n' => key.push('\n'),
                                        b'"' => key.push('"'),
                                        b'\\' => key.push('\\'),
                                        other => key.push(other as char),
                                    }
                                    j += 2;
                                } else {
                                    // Copy one UTF-8 char.
                                    let ch_len = utf8_len(bytes[j]);
                                    key.push_str(&text[j..j + ch_len]);
                                    j += ch_len;
                                }
                            }
                            if !key.is_empty() {
                                keys.insert(key);
                            }
                            i = j;
                            continue;
                        }
                    }
                }
                i += 1;
            }
        }
        // Second pass: header-label arrays (`for key in ["A", "B"]`) feed
        // their literals to t() through a variable — harvest those too.
        for file in &files {
            let text = std::fs::read_to_string(file).expect("read rs file");
            let mut search_from = 0;
            while let Some(rel) = text[search_from..].find("for key in [") {
                let start = search_from + rel;
                let end = text[start..]
                    .find(']')
                    .map(|e| start + e)
                    .unwrap_or(text.len());
                let seg = &text[start..end];
                let seg_bytes = seg.as_bytes();
                let mut j = 0;
                while j < seg_bytes.len() {
                    if seg_bytes[j] == b'"' {
                        j += 1;
                        let mut key = String::new();
                        while j < seg_bytes.len() && seg_bytes[j] != b'"' {
                            if seg_bytes[j] == b'\\' && j + 1 < seg_bytes.len() {
                                j += 2;
                            } else {
                                let ch_len = utf8_len(seg_bytes[j]);
                                key.push_str(&seg[j..j + ch_len]);
                                j += ch_len;
                            }
                        }
                        if !key.is_empty() {
                            keys.insert(key);
                        }
                    }
                    j += 1;
                }
                search_from = end;
            }
        }
        assert!(keys.len() > 300, "scanner found suspiciously few keys");
        let mut missing: Vec<String> = Vec::new();
        for key in &keys {
            if ALLOW_SAME.contains(&key.as_str()) {
                continue;
            }
            for lang in [super::Language::ZhCn, super::Language::ZhTw] {
                if lang.tr(key) == key.as_str() {
                    missing.push(format!("{key} ({lang:?})"));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "untranslated keys: {}",
            missing.join(", ")
        );
    }

    fn utf8_len(first: u8) -> usize {
        if first < 0x80 {
            1
        } else if first >> 5 == 0b110 {
            2
        } else if first >> 4 == 0b1110 {
            3
        } else {
            4
        }
    }

    #[test]
    fn tables_sorted_and_unique() {
        sorted_unique(ZH_CN);
        sorted_unique(ZH_TW);
        assert_eq!(ZH_CN.len(), ZH_TW.len(), "tables out of sync");
    }

    #[test]
    fn no_unreplaced_placeholders() {
        for table in [ZH_CN, ZH_TW] {
            for (k, v) in table {
                for needle in ["{task_id}", "{name}", "{count}", "{err}", "{e}"] {
                    if k.contains(needle) {
                        assert!(
                            v.contains(needle),
                            "translation dropped placeholder {needle} in {k:?}"
                        );
                    }
                }
            }
        }
    }
}
