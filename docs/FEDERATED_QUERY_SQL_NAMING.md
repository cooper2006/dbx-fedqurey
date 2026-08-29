# 联邦查询 SQL 命名规范

## 概述

联邦查询允许跨多个数据库连接执行联合查询。本文档描述了 SQL 命名规范、各数据库类型的具体语法以及自动重写规则。

## 基本语法

联邦查询使用连接名前缀标识表所属的连接：

```sql
-- 3 段式：connection.database.table
SELECT * FROM <连接名>.<数据库名>.<表名> LIMIT 1;

-- 4 段式：connection.database.schema.table（显式指定 schema）
SELECT * FROM <连接名>.<数据库名>.<schema名>.<表名> LIMIT 1;
```

### 连接名匹配规则

- **大小写不敏感**：连接名 `PostgreSQL` 可通过 `postgresql`、`PostgreSQl` 等任意大小写形式引用
- **特殊字符**：含连字符、空格等特殊字符的连接名（如 `doris-Local`）需加双引号：`"doris-Local".freequery.table`
- **去重**：系统自动为含特殊字符的连接名添加双引号，用户无需手动处理

## 数据库类型规范

### PostgreSQL 系

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| PostgreSQL | `pgLocal` | `pgLocal.tpcds.public.item` |
| Redshift | `rsLocal` | `rsLocal.mydb.public.users` |
| Kingbase | `kbLocal` | `kbLocal.mydb.public.orders` |
| Highgo | `hgLocal` | `hgLocal.mydb.public.events` |
| Uxdb | `uxLocal` | `uxLocal.mydb.public.data` |
| Vastbase | `vbLocal` | `vbLocal.mydb.public.logs` |
| GaussDB | `gsLocal` | `gsLocal.mydb.public.metrics` |
| OpenGauss | `ogLocal` | `ogLocal.mydb.public.records` |
| Kwdb | `kwLocal` | `kwLocal.mydb.public.info` |
| Oscar | `osLocal` | `osLocal.mydb.public.detail` |

**默认 schema**: `public`

**常见写法**：
```sql
-- 连接 pgLocal 的 database=tpcds，schema=public
SELECT * FROM pgLocal.tpcds.public.item LIMIT 1;
-- 或省略 schema（自动使用 public）
SELECT * FROM pgLocal.tpcds.item LIMIT 1;
```

**自动重写**：当中间段（database 部分）与连接的实际数据库名相同时，重写时自动丢弃该段，使用默认 schema。
- `pgLocal.tpcds.item` → `public.item`（database `tpcds` 匹配连接数据库名，保留默认 schema `public`）

---

### MySQL 系

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| MySQL | `mySQLocal` | `mySQLocal.shop.orders` |
| Doris | `dorisLocal` | `dorisLocal.freequery.DIM_USER` |
| StarRocks | `srLocal` | `srLocal.mydb.mytable` |
| GoldenDB | `gdLocal` | `gdLocal.bank.account` |
| GBase | `gbaseLocal` | `gbaseLocal.mydb.table1` |
| ManticoreSearch | `mcLocal` | `mcLocal.mydb.documents` |

**默认 schema**: 数据库名本身（无 schema 概念）

**常见写法**：
```sql
-- MySQL：database 即为 schema，可省略第三段
SELECT * FROM mySQLocal.shop.orders LIMIT 1;
-- 显式写全
SELECT * FROM mySQLocal.shop.shop.orders LIMIT 1;
```

**自动重写**：当中间段匹配连接数据库名时，保留 database 段（MySQL 无 schema，database=table 所在 namespace）。
- `mySQLocal.shop.orders` → `shop.orders`（database `shop` 是真实数据库名，保留）

---

### SQL Server

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| SQL Server | `ssLocal` | `ssLocal.mydb.dbo.users` |

**默认 schema**: `dbo`

**常见写法**：
```sql
SELECT * FROM ssLocal.mydb.dbo.users LIMIT 1;
```

---

### Oracle 系

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| Oracle | `oraLocal` | `oraLocal.mydb.MY_SCHEMA.users` |
| OceanBase (Oracle 模式) | `obLocal` | `obLocal.mydb.MY_SCHEMA.orders` |

**默认 schema**: 数据库名（即用户名）

**常见写法**：
```sql
-- 3 段式：连接.schema.表（schema 等同于用户名）
SELECT * FROM oraLocal.mydb.MY_SCHEMA.users LIMIT 1;
```

---

### DB2

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| DB2 | `db2Local` | `db2Local.mydb.SCHEMA_NAME.tables` |

**默认 schema**: 数据库名（即用户名）

---

### 达梦

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| Dameng (达梦) | `dmLocal` | `dmLocal.mydb.SYSDBA.users` |

**默认 schema**: `SYSDBA`

---

### ClickHouse

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| ClickHouse | `ckLocal` | `ckLocal.mydb.default.users` |

**默认 schema**: 数据库名本身

---

### Snowflake

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| Snowflake | `sfLocal` | `sfLocal.mydb.MY_SCHEMA.users` |

**默认 schema**: 数据库名

---

### Hive / Presto / Trino / Spark 系

| 数据库类型 | 连接名 | 示例 |
|-----------|--------|------|
| Hive | `hiveLocal` | `hiveLocal.mydb.default.users` |
| PrestoSQL | `prestoLocal` | `prestoLocal.mydb.default.orders` |
| Trino | `trinoLocal` | `trinoLocal.mydb.default.metrics` |
| Spark | `sparkLocal` | `sparkLocal.mydb.default.events` |
| Kyuubi | `kyuubiLocal` | `kyuubiLocal.mydb.default.data` |
| Impala | `impalaLocal` | `impalaLocal.mydb.default.tables` |
| Databricks | `dbxLocal` | `dbxLocal.mydb.default.results` |
| Kylin | `kylinLocal` | `kylinLocal.mydb.default.cube` |

**默认 schema**: `default`

---

### 其他数据库

| 数据库类型 | 连接名 | 示例 | 默认 schema |
|-----------|--------|------|------------|
| Teradata | `tdLocal` | `tdLocal.mydb.my_table` | 数据库名 |
| Vertica | `vtLocal` | `vtLocal.mydb.public.users` | `public` |
| Exasol | `exLocal` | `exLocal.mydb.SCHEMA.users` | 数据库名 |
| Firebird | `fbLocal` | `fbLocal.mydb.USERS` | 无 schema |
| H2 | `h2Local` | `h2Local.mydb.public.users` | `public` |
| Informix | `ifxLocal` | `ifxLocal.mydb.public.orders` | `public` |
| Tdengine | `tdngLocal` | `tdngLocal.mydb.my_table` | 数据库名 |
| Xugu (虚谷) | `xgLocal` | `xgLocal.mydb.public.users` | `public` |
| YashanDB (亚信) | `ysLocal` | `ysLocal.mydb.public.data` | `public` |
| Sundb | `sdLocal` | `sdLocal.mydb.default.users` | `default` |
| QuestDB | `qdLocal` | `qdLocal.mydb.public.events` | `public` |
| Ignite | `igLocal` | `igLocal.mydb.DEFAULT.users` | 数据库名 |
| Ignite3 | `ig3Local` | `ig3Local.mydb.DEFAULT.records` | 数据库名 |
| BigQuery | `bqLocal` | `bqLocal.project.dataset.users` | dataset 即 schema |
| Rqlite/Turso/D1 | `sqliteLocal` | `sqliteLocal.main.users` | `main` |
| Neo4j | `neoLocal` | `neoLocal.graph.nodes` | 无 schema |
| Cassandra | `cassLocal` | `cassLocal.mykeyspace.mytable` | keyspace 即 schema |
| InfluxDB | `infLocal` | `infLocal.mydb.measurements` | 无 schema |
| VictoriaMetrics | `vmLocal` | `vmLocal.metrics.timeseries` | 无 schema |

---

## 自动重写规则

### 单连接联邦查询重写

当 SQL 中引用的 database 段与连接的实际数据库名相同时，重写时自动丢弃该段，改用连接的默认 schema：

| 用户写入 | 连接配置 | 重写结果 |
|---------|---------|---------|
| `pgLocal.tpcds.item` | db=`tpcds`, default_schema=`public` | `public.item` |
| `mySQLocal.shop.orders` | db=`shop` | `shop.orders`（MySQL 无 schema，保留） |
| `ssLocal.mydb.dbo.users` | db=`mydb`, default_schema=`dbo` | `dbo.users` |
| `hiveLocal.mydb.events` | db=`mydb`, default_schema=`default` | `default.events` |

**注意**：MySQL/Doris 系数据库无 schema 概念，database 段即数据库名，重写时**保留**该段。

### 连接池路由

单连接联邦查询会自动路由到 SQL 中引用连接的连接池，而非标签页的当前连接。例如在 `pgLocal` 标签页执行 `SELECT * FROM mySQLocal.shop.orders`，会自动路由到 `mySQLocal` 的连接池。

## 多连接联邦查询（Calcite 引擎）

多连接查询不经过 Rust 重写，直接转交 Calcite Agent 执行。命名规范：

```sql
-- 跨 MySQL 和 PostgreSQL 的 JOIN
SELECT s.order_id, u.username
FROM mySQLocal.shop.orders s
JOIN pgLocal.tpcds.public.users u ON s.user_id = u.id
LIMIT 10;
```

Calcite Agent 自动处理各数据库的 schema 语义差异。

## 前端辅助工具

### 联邦 SQL 格式化

`apps/desktop/src/lib/federated/federatedFormatter.ts` 提供：
- `formatFederatedSql(sql)` — 格式化时保留联邦前缀
- `analyzeFederatedSql(sql)` — 分析 SQL 中的联邦模式
- `stripFederationPrefixes(sql)` — 去除联邦前缀（用于单连接执行）
- `addFederationPrefixes(sql, connectionMap)` — 添加联邦前缀

### 方言检测

`apps/desktop/src/lib/federated/dialectDetector.ts` 提供：
- `autoDetectDialect(sql)` — 基于 SQL 特征自动检测方言
- `getQuoteCharacter(dbType)` — 获取方言相关的引号字符
- `quoteIdentifier(name)` — 对标识符加引号
- `formatTableReference(connection, database, schema, table)` — 格式化表引用

## 验证联邦查询

```sql
-- 测试单连接 PostgreSQL
SELECT * FROM pgLocal.tpcds.item LIMIT 1;

-- 测试单连接 MySQL
SELECT * FROM mySQLocal.tpcds.store_sales LIMIT 1;

-- 测试多连接 JOIN
SELECT s.ss_ticket_number, i.i_item_desc
FROM mySQLocal.tpcds.store_sales s
JOIN pgLocal.tpcds.item i ON s.ss_item_sk = i.i_item_sk
LIMIT 10;
```

---

*创建日期: 2026-08-20*
*最近更新: 2026-08-21*
*版本: 1.1*
