# Supervisor Arena — 决策清单

> **用法**:
> - ☑ = 已确认
> - ☒ = 已拒绝(带原因)
> - ⚠ = 部分确认,有子项待定
> - ❓ = 未讨论
> - 🆕 = 新增决策(原清单没有)
>
> 每条都附:**倾向** + **理由** + **风险** + **与其他决策的关联**。

---

## A. 核心定位

### A-1 ☑ 系统本质是"工具+软评论"
- **选项 B 落地**:评分 + 轻量软评论(评分时的"附加信息")
- **确认**:不开放讨论区,避免过激讨论
- **理由**:用户偏好明确
- **关联**:F-1、F-9、F-10

### A-2 ☑ 目标用户分层
- **消费者(主)**:考生、择校者
- **消费者(次)**:转专业学生
- **数据源**:在读学生(必须声明)
- **禁区**:**老师完全禁止**参与(避免刷分)
- **关联**:E-1(老师软移除)

### A-3 ☑ 数据公开性(双层墙)
- **未登录**:不可见任何评分
- **基础登录**:综合分 + 雷达图
- **会员**:均值变动曲线 + 共识度 + 置信区间
- **关联**:F-3、F-8(不做导出)

### A-4 ☑ 商业模式
- **部分免费 + 详情收费**
- 基础:免费
- 会员:订阅付费
- **关联**:H-10(支付)、F-8(导出)

### A-5 ☑ 国际化
- **中英双语**
- **关联**:H-5(前端 i18n)

---

## B. 匿名与身份

### B-1 ☑ 匿名粒度
- **A:每次评分独立不关联**
- **理由**:用户说"任何用户无法查看具体评分来源和总评价人数"
- **关联**:B-7(学生身份)

### B-2 ☒ 不显示账号信誉等级
- **关联**:C-7(无信誉)

### B-3 ☑ 邮箱可换 + 同源销毁
- **C**:换邮箱=新账号;若被识别为**同注册来源**,**旧数据销毁**
- **检测**:同 IP + 同学校 + 同行为模式
- **关联**:E-4(同 IP 限制)

### B-4 ☑ 账号注销 + 匿名化保留
- **关联**:G-1(隐私合规)

### B-5 ☑ 多设备登录,继承
- **关联**:E-4(同 IP 限 1 活跃会话)

### B-6 ☑ 开放注册
- **关联**:E-1(注册验证强度)

### B-7 🆕 ☑ 学生身份必填
- **注册时必填**:学科 + 学校
- **选填**:年级
- **用途**:学科相关性加权、软移除检测
- **关联**:C-2(权重)、E-1(老师软移除)

### B-8 🆕 ☑ 评分人数限制
- **基础账号**:同一导师最多 **3 条** 评分
- **会员**:同一导师最多 **10 条** 评分
- **超出处理**:学科相关 → 加权计入(0.3–0.7);完全不相关 → 不计入
- ⚠ **待澄清**:同导师所有维度合计 3/10,还是每个维度限 3/10?

### B-9 🆕 ☑ 评分覆盖规则
- **同一人再次评分 = 覆盖当前值**
- **当前值替换 + 原始数据保留**(用于轨迹)
- **附加信息不公开**

---

## C. 评分模型

### C-1 ☑ 评分量级 = 0–100

### C-2 ☑ 维度权重 = 学科自适应 + 用户共同投票
- 投票门槛:同学科已提交 ≥ 3 条评分
- 通过门槛:≥ 60% 同学科活跃用户
- 冷却期:30 天/维度
- ⚠ **待澄清**:单次变更幅度上限?回滚机制?

### C-3 ☑ 综合分显示分级
- **基础账号**:综合分 + 雷达图
- **会员**:综合分 + 雷达图 + 共识度 + 置信区间 + 均值变动曲线

### C-4 ☑ 维度可扩展,按学科调整
- **B + C 组合**:系统预留 8–12 维度,按需启用
- 按学科启用/调整维度优先级

### C-5 ☑ 允许负分(受控开放)
- **默认不开放**:UI 滑块最低 0
- **触发条件**:同一账号对同一导师**多次降分** + 历时**超过半年**
- **下降限制**:触发后**每过一个月**额度下降 **10**,最高 **-100**
- **公开端约束**:综合分均值**不低于 0**
- ⚠ **待澄清**:会员端可见的"原始数据"是否也不低于 0?

### C-6 ☑ 时间衰减 = 0.1(按学科微调)
- λ = 0.1/年(半衰期 7 年)

### C-7 ☒ 不需要账号信誉公式

### C-8 ☑ 理由分级 = C(NLP 自动识别)
- 扫描"具体年份/数字/URL/事件名"等关键词,自动归级

### C-9 ☒ 不做匹配度

### C-10 🆕 ☑ 评分表单(滑块 + 附加信息)
- **结构**:
  - **6 个维度滑块**(0-100)
  - **每维度滑块下**有"+ 添加附加信息"按钮(可选)
  - **总附加信息文本框**(对整体,可选)
- **附加信息**:全部**可选**;用户**可以只填滑块**
- **仅用于**审核和聚合权重,**不公开**给其他用户
- **理由**:用户明确"评分表单主要由选项滑块构成,每个滑块下可以额外单独创建一个附加信息...还有一个可选的总附加信息对于整体进行评价"
- **关联**:C-1、G-11、G-21

### C-11 🆕 ☑ 均值变动曲线(替代"修改历史")
- **没有"修改历史"概念**
- 替代:**均值变动曲线**(综合统计均值的时间序列)
- 后台每小时计算一次聚合快照,存 `SupervisorAggregateSnapshot`
- **不存储**任何个人级数据
- 会员可见,基础账号不可见
- **不展示**:具体修改者 / 具体修改时间(只到小时)/ 附加信息内容
- **理由**:用户明确"修改历史只有会员可见,严格来说不是修改历史而是综合统计均值数据的历史变动曲线"
- **关联**:B-9、G-21

---

## D. 动态相对修正

### D-1 ❓ 动态相对修正是否要做?
- **当前状态**:用户未明确
- **默认**:按 OUTLINE §5 草案(守恒修正 + 防刷分)
- ⚠ **待澄清**:是默认按 §5 实施,还是延后到 M3+?

---

## E. 反滥用

### E-1 🆕 ☑ 老师账号软移除
- **检测方式**:注册"学校+学科"组合 + 行为模式 + 行为指纹 + 用户举报
- **处理**:静默丢弃其所有评分,**不主动通知**
- **关联**:B-7(学生身份)、E-4(同 IP 限制)

### E-2 ☑ 注册验证强度
- **B:邮箱 + CAPTCHA**

### E-3 ☑ 评分速率限制
- **基础账号**:每天 ≤ 10 条
- **会员**:每天 ≤ 30 条

### E-4 ☑ 同 IP 限制
- **D:同 IP 同时活跃账号限 1 个**

### E-5 ⚠ 仲裁流程
- **当前状态**:基础方案 C(随机抽 N 名高信誉账号)
- ⚠ **待澄清**:老师软移除后是否还有"高信誉账号"概念?信誉无等级后,仲裁如何组织?

### E-6 ☑ 申诉流程
- **B 简化**:被软移除后无显式申诉入口
- ⚠ **待澄清**:是否完全无申诉,还是留邮件申诉?

### E-7 ☑ 黑名单公开
- **B:公开"被软移除账号数"等聚合数据**

### E-8 ☑ 评分前不显示群体信息
- **A:评分前不显示任何群体信息**(符合用户"无法查看总评价人数")

---

## F. UI / UX

### F-1 ☑ 主页信息密度
- **B:中等**(搜索 + Top + 最新动态)

### F-2 ❓ 搜索维度
- **倾向 C:姓名 + 机构 + 学科 + 方向标签**

### F-3 ☑ 登录墙策略
- **C 升级**:未登录不可见评分,登录后基础可见,会员看更多

### F-4 ☑ 移动端 vs 桌面端
- **C:响应式**

### F-5 ❓ 评分表单流程
- **倾向 A:单页滑块 + 文本框 + 提交**

### F-6 ❓ 修改历史 UI
- **已被 C-11 替代**:均值变动曲线(会员可见,聚合级)

### F-7 ☑ 对比功能(任何用户可用)
- **机制**:A vs B 双雷达图(2-5 个导师对比)
- **可用性**:**任何用户可用**
- **理由**:用户明确"提供对比功能,任何用户可用"
- **关联**:A-3

### F-8 ☒ 不做导出功能
- **决策**:**不做**导出功能
- **理由**:用户明确"不做导出功能"
- **影响**:数据完全在线,无离线副本(降低泄露风险)
- **关联**:A-3

### F-9 ☒ 不做通知/订阅

### F-10 ☒ 不做讨论区

### F-11 🆕 ☑ 评价后感谢弹窗
- 提交评分后弹出,显示 `pending_review` 状态 + 审核时间
- **理由**:用户明确"进行评价后会弹出感谢信息"

---

## G. 内容政策 / 法律 / 伦理

### G-1 ☑ 隐私合规
- **A:仅国内,符合个保法**

### G-2 ☑ 敏感词/诽谤处理
- **决策**:**关键词过滤 + 用户举报**
- **机制**:NLP 关键词扫描(评价内容 + 附加信息)+ 任何用户可举报
- **关联**:G-11、G-23

### G-3 ☑ 举报功能(任何用户 → 后台审核)
- **决策**:**任何用户可举报**,举报进入**后台审核流程**
- **范围**:
  - 评价内容违规(诽谤/侮辱/隐私)
  - 附加信息违规
  - 化名条目违规
- **流程**:
  1. 用户点击"举报"按钮
  2. 选择举报类型 + 填写理由
  3. 进入审核员队列
  4. 审核员按 G-11 SLA 处理
- **理由**:用户明确"举报功能任何用户可以进行,然后进入后台审核流程"
- **关联**:G-11

### G-4 ☒ 不做导师自证/反驳
- **理由**:用户明确"导师本人没有自证的说法,因为没有任何信息说明评分的是具体的导师"
- **背景**:化名与真实人名无关,平台主动声明"不对应真人"
- **关联**:G-3、G-13

### G-5 ☒ 不做广告/软文

### G-6 ☑ 跨学科可比性
- **C:学科内对比,跨学科只显示维度画像**

### G-7 🆕 ☑ 评判项目隐私制度
- 评分内容匿名;修改理由不公开;证据可被引用不暴露提交者;数据脱敏

### G-8 🆕 ☑ 敏感信息分级 + 加密制度
- **P0 极度敏感**(邮箱):AES-256-GCM
- **P1 高度敏感**(学校/学科/IP):HMAC-SHA256
- **P2 准敏感**(评分/理由):AES-256-GCM
- **P3 公开数据**:明文
- **密码**:Argon2id
- **传输**:TLS 1.3 + HSTS
- **关联**:B-7、G-1

### G-9 🆕 ❓ 密钥管理(KMS)选型
- **候选 A**:阿里云 KMS(国内合规优先)
- **候选 B**:AWS KMS(国际)
- **候选 C**:HashiCorp Vault(自托管)
- **倾向**:**A 起步**

### G-10 🆕 ❓ 泄露应急响应
- 24h 冻结+轮换,72h 通知+报告;季度演练;>1000 账号依法 72h 报告监管

### G-11 🆕 ☑ 人工审核系统
- 范围:所有新评分 + 所有修改评分
- 状态:`pending_review` → `approved` / `rejected`
- SLA:24h 工作日 / 72h 非工作日
- 审核员:M1 内部 1-3 人;M2+ 高信誉志愿者
- ⚠ **待澄清**:审核员是否需签署保密协议?审核员资质?
- **关联**:G-3(举报)

### G-12 🆕 ☑ 敏感信息禁止(科研内容)
- **P0 严禁**:未发表论文/数据/技术细节/合作项目
- **P1 脱敏**:真名/机构/实验室/项目代号
- **P2 警告**:公开论文/演讲
- **关联**:G-11、G-8

### G-13 🆕 ☑ 导师匿名系统(无关化名)
- **用户最新澄清**:
  - 任何用户可创建导师,**无限制**,**随意命名**,**不一定是真名**
  - 审核员**只审隐私/恶意**,**不查真实性**,**无查询权**
  - 后台映射表约束:**任何化名都不得对应真实存在的人名**
- **三层架构**:
  - 公开层:化名(平台生成,与人名白名单无关)+ 学科 + 学院 + 评分
  - 后台层:用户原始名(任意)→ 平台生成化名(无关),加密,物理隔离
  - 数据收集层:用户提交任意名称
- **化名生成器**:多风格 + 学科融合 + 多字符集
- **风险等级**:🟢 **极低**
- **前置条件**(M7 启动前):律师法律意见书
- **关联**:G-8、G-11、G-12

### G-14 🆕 ☑ k-匿名保护机制
- 规则:同"学科+学院"分类下活跃导师数 < 10 → 整组不显示
- 实现:DB + API 双层校验
- 前端:不区分"暂无数据"和"k-匿名不显示"原因
- **理由**:用户明确"在两个标签确定的同一分类下最少要有10名导师否则不显示"

### G-15 🆕 ☑ 后台原始名映射(无关化名版)
- 存储:`SupervisorNameMapping` 表,AES-256-GCM 加密
- 物理隔离:与公开数据分库/分表
- 不可导出:API/导出/截图/备份**禁止**访问
- 审核员权限:**无查询权**
- **理由**:用户明确"映射只在后台保存,不向用户提供任何查询通道"

### G-16 🆕 ☑ 导师创建流程(无关化名版)
- 用户提交:任意名称 + 学科 + 学院
- 后端去重:`hash(submitted_name) + discipline + college` 查重
- 不存在 → 进入 `pending_review`
- 审核员:只审隐私/恶意,不查映射表
- 通过 → 平台生成无关化名 → 写入映射 → 公开
- 任何用户可创建,**无限制**

### G-17 🆕 ☑ 化名生成器(多风格 + 学科融合 + 多字符集)
- **多风格**:古风 + 自然 + 几何 + 学科融合
- **多字符集**:拉丁 + 希腊(α-ω)+ 数字 + 数学符号
- **生成流程**:随机选词 + 学科符号 + 哈希后缀 + 白名单校验 + 唯一性校验
- **白名单**:每季度更新

### G-18 🆕 ☑ 评价入口
- 按"学科+学院"搜索 → 化名列表 → 点击
- 或:分享化名链接直接访问
- 或:找不到 → 走"创建导师"流程

### G-19 🆕 ☑ 同名条目去重逻辑
- **规则**:同一"用户原始名 + 学科 + 学院" = 同一条目
- **不同学科/学院** = **多个独立档案**
- 例子:"张伟 + 计算机学院"和"张伟 + 历史学院" → 两个独立档案

### G-20 🆕 ☑ 化名重复处理(严格 1-to-1)
- **规则**:化名**严格 1-to-1**;跨学科+学院**不可重用**
- 反例禁止:计算机档案用"α-net-7k2" 不可被历史档案再用

### G-21 🆕 ☑ 重复评价提示
- 触发:同账号对同化名已有评分
- 弹"已评价过"提示(中英)
- **数据无害化处理**:
  - 该账号原始评分**只用于聚合计算**
  - 公开页**不显示**该账号的具体评价轨迹
  - 会员可见的"修改时间线"中该账号记录**脱敏**
- **理由**:用户明确"对于同一用户重复对同一名字进行评价时,会有提示说明:已经进行过评价,并对数据无害化处理"

### G-22 🆕 ☑ 评价修改
- 入口:用户个人页"修改评价"按钮
- 流程:走 B-9 评分覆盖
- 展示:旧值保留(用于轨迹),公开页脱敏展示,会员可见修改时间线但修改者匿名

### G-23 🆕 ☑ 评分表单(滑块+附加信息)
- 见 C-10

### G-24 🆕 ☑ 均值变动曲线(替代修改历史)
- 见 C-11

### G-25 🆕 ☑ 数据保留期永久
- **理由**:用户明确"数据保留期永久"
- **关联**:A-4

### G-26 🆕 ☑ 评分公式可后续修改
- **机制**:公式参数存为配置;M+ 优化时可调整,**不破坏历史数据**
- **历史快照用旧公式,新数据用新公式**
- **理由**:用户明确"评分模型公式可以在后续进行修改"
- **关联**:C-3

### G-27 🆕 ☒ 不做侵权投诉
- **决策**:**不做**
- **理由**:用户明确"侵权投诉应该没有这种说法,因为没有任何实名信息公开"
- **关联**:G-3(举报替代)

### G-28 🆕 ☒ 不做导师自证
- 见 G-4

---

## H. 技术选型

### H-1 ☑ 后端 = Rust
- **理由**:用户明确"后端定为Rust"
- **Rust 优势**:
  - 类型系统 + 编译时 SQL 校验(`sqlx`)能提前捕获大量错误
  - 零成本抽象 + 高性能 + 高并发(适合隐私敏感系统)
  - 内存安全(无 GC,无数据竞争)
  - 加密 crate 生态成熟(`aes-gcm` / `ring` / `rustls` / `argon2`)
- **待讨论**:actix-web / axum / rocket

### H-2 ☑ 数据库 = PostgreSQL
- **理由**:用户明确"数据库确定为PostgreSQL"
- **PostgreSQL 优势**:
  - 关系型 + JSONB(混合数据)
  - 内置 FTS(全文搜索,无需额外服务)
  - 字段级加密 + TDE 支持
  - 成熟稳定 + Rust 生态(`sqlx` / `diesel`)

### H-3 ❓ 存储(证据附件)
- 倾向:**不入库,仅 URL**(配合证据形式)

### H-4 ☑ 搜索引擎 = Postgres FTS
- **理由**:用户明确"引擎用PostgresFTS"
- **优势**:无需额外服务(Elasticsearch/Meilisearch),初期够用
- **后期可升级**:如数据量大,迁移到 Meilisearch

### H-5 ☑ 前端 = React
- **理由**:用户明确"前端采用React"
- **配套**:react-i18next(中英双语)+ Vite(构建)

### H-6 ❓ 部署
- 候选:Vercel / 自托管(Docker)/ Cloudflare

### H-7 ❓ 监控
- 倾向:**无(初期)** → Sentry 备选

### H-8 ❓ 邮件服务
- 倾向:**不发送邮件(初期)**

### H-9 ☑ CI/CD = GitHub Actions

### H-10 ❓ 支付渠道
- 候选:Stripe / 支付宝 / 微信

### H-11 ☑ DB driver = tokio-postgres + deadpool-postgres(Plan B)
- **选项**:
  - Plan A:降级到 `sqlx = "0.7"`(确认可工作,但版本老)
  - Plan B:用 `tokio-postgres` + `deadpool-postgres` 直连(版本新,绕开 sqlx 0.8 bug)
  - **确认**:Plan B
- **理由**:
  - sqlx 0.8 的 `ErrorResponse` parser 在 Alpine musl PostgreSQL 镜像下会因 `lc_messages` 编码问题拒绝非 UTF-8 字节(即使显式设 `lc_messages=C` 也无济于事)
  - 调试耗时:4 次 commit 尝试绕过(0.8 → 0.7 → after_connect hook → 双重保险)全部失败
  - 用户明确拒绝降级:希望保留 0.8 时代的依赖树
- **实现**:
  - `tokio-postgres = "0.7"`(原 PostgreSQL 协议库,无 SQL 解析层,所以没有这个 bug)
  - `deadpool-postgres = "0.14"`(async pool,避免连接泄漏)
  - `deadpool = "0.12"`(显式依赖,`db.rs` 直接用 `deadpool::managed::*` 配 PoolConfig)
  - 手写 migration runner:读 `./migrations/*.sql`,按文件名排序,track `_migrations` 表
- **损失**:
  - 失去 `sqlx::query!` 的编译时 SQL 校验(但能接受 — migrations 是静态 SQL,后期用 `sqlx-cli prepare` 校验可选)
  - 失去 `sqlx::FromRow` 派生(自己写 `From<&Row>` 或在 repo 层定义)
- **关联**:H-1(后端 Rust)、H-2(数据库 PostgreSQL)

### H-12 ☑ Postgres 端口 = 5433(避免本地 PG 冲突)
- **问题**:Windows host 上有 `postgresql-x64-16` 服务(自动启动),绑 `0.0.0.0:5432`
- **影响**:Docker `5432:5432` port mapping 静默被本地服务 shadow,host 上 `localhost:5432` 实际连到本地 PG,密码/dataset 全错;TCP 握手能完成但 PG protocol 协商会卡死(因为本地 PG 用 scram-sha-256 而 docker 里 db user 的密码哈希对不上)。从 host 看像"hang"或"password auth fail"
- **修法**:docker-compose 改 `"5433:5432"`,`DATABASE__URL` 同步用 5433
- **未来**:生产用 Unix socket 或 k8s Service,不会撞
- **关联**:H-11、I-1(本地开发体验)

### H-13 ☑ env 变量命名 = `__` 双下划线嵌套
- **问题**:`config::Environment::default().separator("__")` 意味着 `DATABASE__URL` 才映射到 `database.url` 嵌套 key;单下划线的 `DATABASE_URL` 不会被识别,set_default 默认值生效
- **隐藏 bug 模式**:默认值碰巧能用(比如 `localhost:5432` 跟旧 docker 一致),改动 `DATABASE_URL` 切端口完全无效 — 程序继续用默认端口运行
- **修法**:全部 env 变量改成 `__` 双下划线(`DATABASE__URL`, `AUTH__JWT__SECRET` 等),匹配 `config.rs` 的 separator
- **判据**:改 .env 后 cargo run 输出 URL 没变,基本就是这个 bug
- **关联**:H-11、config.rs

### H-14 ☑ M3 crypto 模块 = AES-256-GCM + HMAC-SHA256 + Argon2id + LocalKeyStore
- **范围**(对齐 G-8 / OUTLINE §7.9):
  - `crypto::aes` — AES-256-GCM(认证加密,可还原),P0/P2 字段
  - `crypto::hmac` — HMAC-SHA256(单向),P1 字段
  - `crypto::argon2` — Argon2id(密码专用)
  - `crypto::keystore::LocalKeyStore` — 启动期从 env 加载 2 个 32-byte key,AES 用 + HMAC 用
- **输出格式**:
  - AES blob:`nonce(12) || ciphertext || tag(16)`(直接存 BYTEA)
  - HMAC:小写 hex 字符串(64 chars,存 VARCHAR(64) 或 BYTEA 都行)
  - Argon2:PHC 字符串(`$argon2id$v=19$m=...$t=...$p=...$salt$hash`)
- **依赖**:`aes-gcm = "0.10"` + `aead = "0.5"` + `hmac = "0.12"` + `sha2 = "0.10"` + `argon2 = "0.5"` + `rand_core = "0.6"` + `getrandom = "0.2"` + `hex = "0.4"` + `zeroize = "1"`(已选 0.10/0.5/0.6/0.2 是因为 aes-gcm 0.10 / aead 0.5 锁定这套;`rand 0.8` 也直接依赖但跟 aead 0.5 走 rand_core 0.6 路径)
- **API**:
  - `aes::encrypt/decrypt(&[u8;32], &[u8], Option<&[u8]>)` — AAD 可选
  - `aes::encrypt_str/decrypt_str` — 字符串便捷包装
  - `hmac::hash_str/hash_str_with_salt` — hex 字符串输出
  - `argon2::hash_password/verify_password` — PHC 格式
  - `LocalKeyStore::from_config(&EncryptionConfig)` / `from_raw` / `field_key()` / `hmac_key()` / `key_id()`
- **测试**:28 unit tests 全过(round-trip / AAD mismatch / 篡改检测 / nonce 唯一性 / 错误密钥 / PHC verify / 错误密码 / 错 hex / 错长度 / Debug 不泄露密钥)
- **判据**:任何"敏感字段怎么存"的问题,直接看 G-8 表 + 本模块 API;调用方不应重新发明加密逻辑
- **关联**:G-8、H-1、OUTLINE §7.9

### H-15 ☑ AAD = 字段名绑定(防止 cross-column replay)
- **策略**:AES 加密时,推荐传 `Some(b"column_name")` 当 AAD;解密时**必须传相同 AAD** 才通过
- **效果**:`accounts.email_enc` 列里偷出的密文,无法被塞进 `accounts.phone_enc` 列(因为 AAD 不同,GCM 认证失败)
- **默认**:helper 接受 `Option<&[u8]>`,业务层在 encrypt/decrypt 时显式指定列名
- **判据**:任何 P0/P2 字段的 encrypt 调用,都加 `Some(b"accounts.email".as_bytes())` 形式
- **关联**:H-14、OUTLINE §7.9.1

### H-16 ☑ 密钥轮换策略 = 单 key 版本,M6 接 KMS
- **当前(M3)**:1 个 field key + 1 个 hmac key 加载到 `LocalKeyStore`,无版本号
- **轮换**:靠 `ENCRYPTION__KEY_ROTATION_DAYS=90` 提醒(operator 流程);不强制
- **限制**:轮换时**必须重加密全部密文**(因为没有 key id 跟 ciphertext 一起存);这就是为什么 M3 不强制轮换
- **M6 计划**:`KmsKeyStore` 接 AWS KMS / 阿里云 KMS;密文格式变成 `version(1) || nonce(12) || ciphertext || tag(16)`,version 选 key;老 key 不删除,允许 decrypt 旧数据
- **判据**:M3 阶段不要实现"key id 嵌入密文"格式,等 M6 一次性上
- **关联**:H-14、M6(安全加固里程碑)

### H-17 ☑ Dev placeholder = `deadbeef*` 前缀检测
- **约定**:开发期 `.env` 里的 `ENCRYPTION__FIELD_KEY` 和 `ENCRYPTION__HMAC_SALT_KEY` 以 `deadbeef` 开头(经典 hex 占位符模式,有效 hex 字符)
- **检测**:`LocalKeyStore::from_config` 检查任一 key 是否以 `deadbeef` 开头,触发 startup warning + `key_id = "dev-placeholder"`
- **生产检测**:`key_id = "local:<4-byte-fingerprint>"`,日志可关联"哪个 key 版本加密的"而不泄露 key
- **未来 KMS**:`KmsKeyStore` 直接拿 KMS 提供的 key ARN / alias 当 key_id
- **修法**:生产前 `openssl rand -hex 32` 生成,基本不可能以 `deadbeef` 开头(8 个特定 hex 字符概率 = 1/2^32)
- **关联**:H-14、env.example

### H-18 ☑ M4 account module scope
- **路由**:
  - `POST /auth/register` — 创建账号(邮箱 + 密码 + 学科 + 学校 + 可选 grade)
  - `POST /auth/login` — 邮箱 + 密码 → access token
  - `GET  /auth/me` — Bearer token → 当前 account 公开信息
- **加密**:
  - `email_enc` AES-256-GCM(`Some(b"accounts.email_enc")` AAD)
  - `email_hash` / `discipline_hash` / `institution_hash` HMAC-SHA256(32 bytes → BYTEA)
  - `grade_enc` AES-256-GCM (P2,可选)
  - `password_hash` Argon2id PHC 字符串 (TEXT)
- **JWT**:HS256,15 min access TTL,**无 refresh** (M5 再加)
- **关联**:G-8、OUTLINE §7.1/§7.10

### H-19 ☑ Password policy (NIST SP 800-63B §5.1.1.2 风格)
- **要求**:
  - 长度 ≥ 12 字符(ASCII 字节)
  - 必须含至少 1 字母 + 1 数字
  - 不强制大写/符号(避免 `Password1!` 这种可预测模式)
- **不在 M4 实现**:zxcvbn 强度检测、breached password list(Have I Been Pwned API)、密码过期(都列 M5+ 改进)
- **M1-3 已经过 Argon2id 加密**(m=19456 KiB, t=2, p=1,OWASP 默认),hash 时间 ~50ms 即可顶住批量破解
- **关联**:H-18

### H-20 ☑ Email validation = 轻量格式检查(M4) + 真实验证(M5+)
- **当前 (M4)**:非空、≤254 字符、唯一 `@`、domain 至少 1 `.`、无 whitespace
- **不验**:
  - MX 记录(性能 + 隐私)
  - 一次性邮箱(privacy.duckduckgo.com 等列表,放 M5 决策)
  - 真实验证邮件(无 SMTP 服务,M4 不发)
- **M5+**: 加 `email_verified bool` 列 + SMTP 集成 + 24h token 链接
- **关联**:H-18

### H-21 ☑ /auth/login = opaque error 防 enumeration
- **"用户不存在" 和 "密码错" 返回相同的 `401 unauthorized` + 相同 body**(`{"error":"unauthorized","message":"invalid credentials"}`)
- **缓解**:rate limit `RATE_LIMIT__LOGIN_PER_MIN=5` / IP(M5 用 Redis, M4 用 in-memory sliding window)
- **不返回**:不返回"账号已 ban"细节(用 `403 AccountUnavailable` 但消息统一"unavailable")
- **关联**:H-18、F-3

### H-22 ☑ /auth/me 字段最小化 (F-3 数据双层墙)
- **返回**:`account_id`, `tier`, `joined_at`
- **不返回**:email / discipline / institution / grade(都是 P0/P1 敏感)
- **理由**:out of scope for M4,业务上也没人需要 `/auth/me` 看自己邮箱(用户知道自己邮箱)
- **未来**:加 `GET /auth/me/settings` 单独 endpoint 返回 P0 解密字段(也是用户主动请求)
- **关联**:H-18、F-3

### H-23 ☑ M5 alias generator 算法
- **核心**:deterministic by seed — 同 `(submitted_name, discipline, college)` 三元组必产同一化名
- **算法**:
  ```
  seed = HMAC-SHA256(hmac_key, "alias:" + name + "|" + disc + "|" + coll + "|salt:N")
  rng  = SplitMix32(seed[0..4])
  style   = pick_style(rng.next())  // 50% 学科融合 / 20% 自然 / 15% 文学 / 15% 几何
  words   = pick_words(style, rng)
  suffix  = 3-char [0-9a-z] (DisciplineFused) 或 6-char hex (Geometric)
  alias   = combine(style, words, suffix)
  ```
- **白名单检查**:生成后查人名表(starter 4172 条,目标 10000+),命中则 `salt += 1` 重试,最多 32 次
- **不可逆**:HMAC key 在服务端,旁观者无法从化名反推输入;攻击者拿一个 alias 不能 derive 其他 (name, disc, coll) 组合
- **1-to-1 强制**:
  - 同一 (name, disc, coll) 必同 alias(算法决定性)
  - 不同 (disc, coll) 必不同 alias(算法 + DB UNIQUE 双重保险)
- **风格分布**:50% 学科融合(满足 OUTLINE §7.10.3 rule 2 强调的"必须多风格 + 学科融合"),剩余三档均分
- **判据**:
  - 测试 `deterministic_for_same_input` 锁死决定性
  - 测试 `never_collides_with_whitelist_in_1000_attempts` 锁死白名单不命中
  - 测试 `all_styles_appear_in_a_large_sample` 锁死 4 风格多样性
- **关联**:G-13、OUTLINE §7.10.3、§7.10.4

### H-24 ☑ M5 alias 词库 = 嵌入常量 (编译时 include)
- **形式**:`const LITERARY_WORDS: &[&str] = &[ ... ];` — 全部 hardcode 在 `words.rs`
- **理由**:
  - 二进制自包含,无部署期 asset 装配
  - 词库随 code 版本化,任何修改都是 code change
  - 单元测试能 pin 词库大小(添加新词会失败,提示扩词库的同步动作)
- **当前规模**:
  - 文学 27 + 文学 title 8 = 35
  - 自然 25
  - 几何 prefix 12
  - 希腊字母 26
  - 数学符号 11
  - 6 学科门类 × ~8 template = ~48 discipline-fused
  - **可寻址化名空间 ~ 1.35 × 10^5 base components × 4 风格 × 36^3 suffix ≈ 10^9**
- **增长**:
  - M5b:补全 6 学科门类到 ~20 template each(总数 ~120)
  - M6:加入 modern / sci-fi 风格(更分散)
  - M7 上线前:验证跨学科覆盖齐全
- **关联**:H-23、words.rs

### H-25 ☑ M5 人名白名单 = starter set 4172 条 + 增长路线图
- **当前规模**:
  - 100 个常见中文姓氏 × 40 个常见单字 + 双字名 → ~4 200 条 "姓+名" 组合
  - 40 个独立单字名("伟" / "芳" / "娜" 等)
  - 128 个英文 given name + surname
  - **合计 4 172 条**
- **starter 阶段可接受**:测试 `never_collides_with_whitelist_in_1000_attempts` 验证 1000 个 (name, disc, coll) 组合零命中;32 次 retry 上限给出 < 10^-39 残留碰撞率
- **增长路线**:
  - M5b:扩到 ~10 000 条(全 百家姓 + 公安部姓氏普查 + 常见 1000 双字名)
  - M6:接公开数据集(US Census surname data, China 2019 census)
  - M7 (法律门):律师评估白名单覆盖度,不达标不发公测
- **判据**:`whitelist_size_is_documented` 测试 + `whitelist_is_lowercased` 测试
- **关联**:H-23、OUTLINE §7.10.3 rule 1、§7.10.7

### H-26 ☑ M5b supervisor service/repo/handler 范围
- **路由**:
  - `POST /supervisors/request` — authed user 提交 (submitted_name, discipline, college)
  - `GET  /supervisors/by-alias/{alias}` — 公开视图,带 k-anon gating
  - `GET  /supervisors/review/queue` — reviewer 看 pending 列表
  - `POST /supervisors/review/{id}` — reviewer approve | reject
- **流程**:
  1. 验证:non-empty + length caps + discipline/college 在 lookup 表
  2. Dedup-by-hash 三元组 (name_hash, disc_hash, coll_hash) 查 supervisor_name_mappings
  3. 已存在 → 返回已有 alias (status: deduplicated, request_id: 0000...)
  4. 不存在 → AES 加密 + 生成 alias (deterministic) + 入 pending_review 队列
  5. Reviewer approve → 单事务:INSERT supervisor (status='approved') + INSERT mapping + UPDATE request (approved, resolved_supervisor_id) + 重算 k_count
  6. Reviewer reject → UPDATE request (rejected, notes)
- **k-anonymity 阈值 = 10** (写死;生产可改 env)
  - approved + k_count >= 10 → visible: true (公开)
  - approved + k_count < 10  → visible: false (row 存在但隐藏)
  - 不是 approved                → visible: false
- **关联**:G-14、H-23、OUTLINE §7.10.4

### H-27 ☑ Dedup 时刻 = approve 之后(不是 submit 时)
- **当前**:`find_mapping_by_dedup` 只查 `supervisor_name_mappings`,该表在 approve 后才有行
- **行为**:同一 (name, disc, coll) 重复 submit:
  - 在 approve 之前:创建 N 条 pending_review 记录(每条都是同样的 alias 因为 deterministic)
  - 在 approve 之后:第 1 次查到 mapping,后续全部 dedup 返回相同 alias
- **Trade-off**:符合 OUTLINE §7.10.4 step 2(查映射表),schema 简洁
- **M5c 改进**:加 `pending_request_dedup` 检查(create_request 前先查 supervisor_creation_requests 的 (name_hash, disc_hash, coll_hash)),避免 reviewer 看到 50 条 pending 张伟
- **关联**:H-26

### H-28 ☑ PG 参数显式 cast (`::text` / `::uuid` / `::bytea`)
- **问题**:tokio-postgres 传 `&[&str]` / `&[&Uuid]` / `&[&[u8]]` 时,PG 经常报 `42P18 could not determine data type of parameter $1`,因为 wire-protocol 层无法 infer param 类型
- **修法**:所有 SQL placeholders 加显式 cast
  - `WHERE code = $1::text` (text 字段)
  - `WHERE id = $1::uuid` (uuid 字段)
  - `WHERE submitted_name_hash = $1::bytea` (bytea 字段)
  - `INSERT ... VALUES ($1::text, $2::text, $3::text)` 等等
- **额外坑**:
  - `i64` 不会自动 narrow 到 `INTEGER` (i32) — 必须显式 `::int` 或改类型
  - placeholder 比 params 多 → PG 报 $N 缺失;比 params 少 → 报 $N 不可 infer
  - 错误 "error serializing parameter 0" 通常是 type 错配(不是 placeholder 缺失)
- **判据**:`logs/log_min_duration_statement=200ms` + docker `log_statement=all` 看到真实 SQL + params
- **关联**:H-26、§7.10.1 物理隔离

### H-29 ☑ M5b created_by = submitter_id(不是 reviewer_id)
- **问题**:首次实现把 `reviewer_id` 写进 supervisor_name_mappings.created_by(语义错 — created_by 是"创建者",应该是 submitter)
- **修法**:approve_request 多传一个 `submitter_id` 参数,INSERT mapping 时用它
- **reviewer_id** 仍记在 supervisor_creation_requests.reviewer_id(独立审计字段)
- **关联**:G-15(物理隔离审计)

---

## I. 运营

### I-1 ❓ 冷启动
- 倾向:邀请种子用户创建

### I-2 ❓ 推广
- 倾向:学术社区(但项目与学术无关,可能改成技术社区)

### I-3 ☑ 不做"机构"层

### I-4 ☒ 不做学术合作
- **理由**:用户明确"不做学术合作,这个项目根本和学术无关"
- **关联**:A-4(商业模式)

### I-5 ☑ 国际化 = 中英双语

### I-6 ❓ 公开 API
- 倾向:开放只读 API

---

## J. 可演化性

### J-1 ☑ 数据保留期 = 永久
### J-2 ☑ 评分模型可升级
### J-3 ❓ 修正规则可调整
- 倾向 B:规则配置化
### J-4 ❓ 数据库迁移策略
- 倾向 B:用 migration tool
### J-5 ❓ 向后兼容性
- 倾向 B:API 版本化

---

## 决策追踪格式

回复时可任选其一:
- **简洁模式**: "同意 D-1 默认, B-8 是同导师合计, A-4 会员权益包括:轨迹/共识/置信/理由/对比/导出"
- **完整模式**: 对每条给 1-2 句解释
- **分批模式**: "先聊 D 和 B-8/B-9"

---

## 仍需澄清的关键点(11 个)

1. **D-1 动态相对修正** — 是默认按 OUTLINE §5 实施,还是延后?
2. **B-8 评分人数限制** — 同导师所有维度合计 3/10,还是每个维度限 3/10?
3. **A-1 软评论** — 仅"附加信息"=软评论,还是另有短评?
4. **E-1 软移除老师** — 具体检测规则(同校+同 IP+行为模式)的判定阈值?
5. **A-4 会员权益细节** — 详情 = 哪些?(对比已开放/轨迹/共识度/置信区间;导出不做)
6. **C-2 权重投票** — 单次变更幅度上限?生效后是否回滚机制?
7. **C-5 负分公开** — 会员端可见的"原始数据"是否也不低于 0?
8. **G-9 KMS 选型** — 阿里云 / AWS / Vault?
9. **G-10 泄露响应 SLA** — 24h 冻结 + 72h 通知是否够?需不需要更快?
10. **G-13 法律咨询** — 何时做?内部律师还是外部律所?预算?⚠ **M7 启动前必须完成**
11. **G-11 审核员资质** — 内部/志愿者/外包?审核员是否需签署保密协议?
12. **E-5 仲裁流程** — 老师软移除后,仲裁如何组织?
13. **E-6 申诉流程** — 完全无申诉,还是留邮件申诉?
14. **F-2 搜索维度** — 姓名 + 机构 + 学科 + 方向标签
15. **F-5 评分表单流程** — 单页滑块 + 文本框 + 提交?
16. ~~H-1 后端~~ — ✅ Rust 已确认
17. ~~H-2 数据库~~ — ✅ PostgreSQL 已确认
18. ~~H-4 搜索引擎~~ — ✅ Postgres FTS 已确认
19. ~~H-5 前端~~ — ✅ React 已确认
20. ~~I-4 学术合作~~ — ✅ 不做 已确认
21. **H-1 Rust Web 框架** — actix-web / axum / rocket?
22. **H-1 Rust ORM** — sqlx / diesel / sea-orm?
23. **H-6 部署** — Vercel / 自托管(Docker) / Cloudflare?
24. **H-10 支付渠道** — Stripe / 支付宝 / 微信?
25. **I-1 冷启动** — 邀请种子 / 公开?
26. **I-2 推广** — 渠道?
27. **I-6 公开 API** — 开放只读 / 不开放?
