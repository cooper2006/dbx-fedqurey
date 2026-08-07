-- ============================================================
-- 联邦查询测试 SQL - PostgreSQL (pgLocal) vs Doris (dorisLocal)
-- tpcds 数据库：5张表（customer, date_dim, item, store, store_sales）
-- ============================================================

-- ============================================
-- 一、单表查询（分别对两个数据源执行）
-- ============================================

-- T1: 基础SELECT - 查看 customer 表全部数据
SELECT * FROM customer;

-- T2: 条件过滤 - 按顾客ID查询
SELECT * FROM customer WHERE c_customer_sk = 1;

-- T3: 模糊匹配 - 查找姓名为 Alice 的客户
SELECT * FROM customer WHERE c_first_name = 'Alice';

-- T4: 日期范围查询
SELECT * FROM date_dim WHERE d_year = 2024 AND d_quarter_name = '2024Q1';

-- T5: 类别筛选
SELECT i_item_sk, i_item_id, i_item_desc, i_brand, i_category, i_current_price
FROM item
WHERE i_category = 'Electronics'
ORDER BY i_current_price DESC;

-- T6: 门店城市筛选
SELECT * FROM store WHERE s_city IN ('Shanghai', 'Beijing') ORDER BY s_store_sk;

-- T7: 销售金额排序
SELECT ss_ticket_number, ss_item_sk, ss_quantity, ss_sales_price, ss_ext_sales_price, ss_net_profit
FROM store_sales
ORDER BY ss_ext_sales_price DESC
LIMIT 10;

-- ============================================
-- 二、多表 JOIN 查询
-- ============================================

-- J1: 简单两表JOIN - 销售记录 + 商品信息
SELECT 
    s.ss_ticket_number,
    s.ss_sold_date_sk,
    s.ss_quantity,
    s.ss_ext_sales_price,
    i.i_item_desc,
    i.i_brand,
    i.i_category
FROM store_sales s
JOIN item i ON s.ss_item_sk = i.i_item_sk
ORDER BY s.ss_ext_sales_price DESC
LIMIT 10;

-- J2: 三表JOIN - 销售 + 商品 + 客户
SELECT 
    s.ss_ticket_number,
    c.c_first_name,
    c.c_last_name,
    c.c_email_address,
    i.i_item_desc,
    i.i_brand,
    s.ss_quantity,
    s.ss_ext_sales_price,
    s.ss_net_profit
FROM store_sales s
JOIN customer c ON s.ss_customer_sk = c.c_customer_sk
JOIN item i ON s.ss_item_sk = i.i_item_sk
ORDER BY s.ss_net_profit DESC
LIMIT 10;

-- J3: 四表JOIN - 销售 + 商品 + 客户 + 日期
SELECT 
    d.d_date,
    d.d_year,
    d.d_quarter_name,
    d.d_month_name,
    c.c_first_name,
    c.c_last_name,
    i.i_item_desc,
    i.i_brand,
    s.ss_quantity,
    s.ss_ext_sales_price,
    s.ss_net_profit
FROM store_sales s
JOIN customer c ON s.ss_customer_sk = c.c_customer_sk
JOIN item i ON s.ss_item_sk = i.i_item_sk
JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
ORDER BY d.d_date DESC
LIMIT 15;

-- J4: 五表全JOIN
SELECT 
    d.d_date,
    d.d_quarter_name,
    d.d_month_name,
    c.c_customer_id,
    c.c_first_name || ' ' || c.c_last_name AS customer_name,
    i.i_item_id,
    i.i_item_desc,
    i.i_brand,
    i.i_category,
    s.ss_quantity,
    s.ss_ext_sales_price,
    s.ss_net_profit
FROM store_sales s
JOIN customer c ON s.ss_customer_sk = c.c_customer_sk
JOIN item i ON s.ss_item_sk = i.i_item_sk
JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
ORDER BY s.ss_ext_sales_price DESC
LIMIT 20;

-- ============================================
-- 三、聚合分析查询
-- ============================================

-- A1: 按年份统计销售总额和总利润
SELECT 
    d.d_year,
    COUNT(*) AS total_orders,
    SUM(s.ss_ext_sales_price) AS total_sales,
    SUM(s.ss_net_profit) AS total_profit,
    AVG(s.ss_ext_sales_price) AS avg_sale_price
FROM store_sales s
JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
GROUP BY d.d_year
ORDER BY d.d_year;

-- A2: 按季度统计销售业绩
SELECT 
    d.d_year,
    d.d_quarter_name,
    COUNT(*) AS order_count,
    SUM(s.ss_ext_sales_price) AS quarter_sales,
    SUM(s.ss_net_profit) AS quarter_profit
FROM store_sales s
JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
GROUP BY d.d_year, d.d_quarter_name
ORDER BY d.d_year, d.d_quarter_name;

-- A3: 按商品类别统计销售额
SELECT 
    i.i_category,
    i.i_brand,
    COUNT(*) AS items_sold,
    SUM(s.ss_quantity) AS total_quantity,
    SUM(s.ss_ext_sales_price) AS category_sales,
    SUM(s.ss_net_profit) AS category_profit
FROM store_sales s
JOIN item i ON s.ss_item_sk = i.i_item_sk
GROUP BY i.i_category, i.i_brand
ORDER BY category_sales DESC;

-- A4: 按客户统计消费情况
SELECT 
    c.c_customer_id,
    c.c_first_name,
    c.c_last_name,
    COUNT(*) AS purchase_count,
    SUM(s.ss_quantity) AS total_items,
    SUM(s.ss_ext_sales_price) AS total_spent,
    SUM(s.ss_net_profit) AS total_profit_from_customer
FROM store_sales s
JOIN customer c ON s.ss_customer_sk = c.c_customer_sk
GROUP BY c.c_customer_id, c.c_first_name, c.c_last_name
ORDER BY total_spent DESC;

-- A5: 按城市和门店统计销售额
SELECT 
    st.s_city,
    st.s_store_name,
    COUNT(*) AS order_count,
    SUM(s.ss_ext_sales_price) AS city_sales,
    SUM(s.ss_net_profit) AS city_profit
FROM store_sales s
JOIN store st ON s.ss_store_sk = st.s_store_sk
GROUP BY st.s_city, st.s_store_name
ORDER BY city_sales DESC;

-- ============================================
-- 四、子查询与复杂逻辑
-- ============================================

-- SQ1: 查询销售额最高的商品
SELECT 
    i.i_item_desc,
    i.i_brand,
    i.i_current_price,
    total_qty.total_quantity,
    total_sales.ext_sales
FROM item i
JOIN (
    SELECT ss_item_sk, SUM(ss_quantity) AS total_quantity
    FROM store_sales
    GROUP BY ss_item_sk
) total_qty ON i.i_item_sk = total_qty.ss_item_sk
JOIN (
    SELECT ss_item_sk, SUM(ss_ext_sales_price) AS ext_sales
    FROM store_sales
    GROUP BY ss_item_sk
) total_sales ON i.i_item_sk = total_sales.ss_item_sk
ORDER BY total_sales.ext_sales DESC
LIMIT 10;

-- SQ2: 查询高价值客户（消费超过平均消费额的客户）
WITH customer_spend AS (
    SELECT 
        c.c_customer_id,
        c.c_first_name,
        c.c_last_name,
        SUM(s.ss_ext_sales_price) AS total_spend,
        COUNT(*) AS purchase_count
    FROM store_sales s
    JOIN customer c ON s.ss_customer_sk = c.c_customer_sk
    GROUP BY c.c_customer_id, c.c_first_name, c.c_last_name
)
SELECT *
FROM customer_spend
WHERE total_spend > (SELECT AVG(total_spend) FROM customer_spend)
ORDER BY total_spend DESC;

-- SQ3: 查询每个季度的畅销品类TOP3
WITH quarterly_sales AS (
    SELECT 
        d.d_year,
        d.d_quarter_name,
        i.i_category,
        SUM(s.ss_ext_sales_price) AS category_sales
    FROM store_sales s
    JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
    JOIN item i ON s.ss_item_sk = i.i_item_sk
    GROUP BY d.d_year, d.d_quarter_name, i.i_category
),
ranked AS (
    SELECT *,
        ROW_NUMBER() OVER (
            PARTITION BY d_year, d_quarter_name 
            ORDER BY category_sales DESC
        ) AS rank
    FROM quarterly_sales
)
SELECT d_year, d_quarter_name, i_category, category_sales
FROM ranked
WHERE rank <= 3
ORDER BY d_year, d_quarter_name, rank;

-- ============================================
-- 五、窗口函数与高级分析
-- ============================================

-- W1: 计算客户的累计消费和消费排名
SELECT 
    c.c_customer_id,
    c.c_first_name,
    c.c_last_name,
    SUM(s.ss_ext_sales_price) AS total_spend,
    RANK() OVER (ORDER BY SUM(s.ss_ext_sales_price) DESC) AS spending_rank,
    PERCENT_RANK() OVER (ORDER BY SUM(s.ss_ext_sales_price)) AS spending_percentile
FROM customer c
JOIN store_sales s ON c.c_customer_sk = s.ss_customer_sk
GROUP BY c.c_customer_id, c.c_first_name, c.c_last_name
ORDER BY total_spend DESC;

-- W2: 计算每个商品的移动平均价格（按时间序列）
SELECT 
    d.d_date,
    i.i_item_desc,
    s.ss_sales_price,
    AVG(s.ss_sales_price) OVER (
        PARTITION BY i.i_item_sk 
        ORDER BY d.d_date 
        ROWS BETWEEN 2 PRECEDING AND CURRENT ROW
    ) AS moving_avg_price
FROM store_sales s
JOIN item i ON s.ss_item_sk = i.i_item_sk
JOIN date_dim d ON s.ss_sold_date_sk = d.d_date_sk
ORDER BY i.i_item_sk, d.d_date;

-- ============================================
-- 六、统计信息收集
-- ============================================

-- ST1: 数据分布统计
SELECT 
    'store_sales' AS table_name,
    COUNT(*) AS row_count,
    COUNT(DISTINCT ss_customer_sk) AS unique_customers,
    COUNT(DISTINCT ss_item_sk) AS unique_items,
    COUNT(DISTINCT ss_store_sk) AS unique_stores,
    COUNT(DISTINCT ss_sold_date_sk) AS unique_dates,
    MIN(ss_sold_date_sk) AS min_date_sk,
    MAX(ss_sold_date_sk) AS max_date_sk,
    SUM(ss_ext_sales_price) AS total_revenue,
    AVG(ss_ext_sales_price) AS avg_sale_price,
    MAX(ss_ext_sales_price) AS max_single_sale
FROM store_sales;

-- ST2: 客户购买行为分析
SELECT 
    c.c_first_name,
    c.c_last_name,
    COUNT(DISTINCT s.ss_item_sk) AS unique_items_bought,
    COUNT(*) AS total_purchases,
    SUM(s.ss_quantity) AS total_quantity,
    SUM(s.ss_ext_sales_price) AS total_spent,
    AVG(s.ss_ext_sales_price) AS avg_transaction_value
FROM customer c
LEFT JOIN store_sales s ON c.c_customer_sk = s.ss_customer_sk
GROUP BY c.c_customer_sk, c.c_first_name, c.c_last_name
ORDER BY total_spent DESC;
