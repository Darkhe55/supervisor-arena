-- 种子数据:常用学科 + 学院
-- 学科(用户注册时声明的"专业方向")
-- 学院(用户注册时声明的"学院",用于导师条目"学科+学院"分类)

-- ===== 学科(disciplines) =====
-- 用 JSONB lookup 表(支持 i18n)
CREATE TABLE disciplines (
    code TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    name_en TEXT NOT NULL,
    category TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_disciplines_active ON disciplines(is_active) WHERE is_active;

-- ===== 学院(colleges) =====
CREATE TABLE colleges (
    code TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    name_en TEXT NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_colleges_active ON colleges(is_active) WHERE is_active;

-- ===== 种子数据 =====

-- 学科(初版 10 个常见学科,后续可扩展)
INSERT INTO disciplines (code, name_zh, name_en, category) VALUES
    ('CS',      '计算机科学与技术', 'Computer Science',           'engineering'),
    ('SE',      '软件工程',         'Software Engineering',       'engineering'),
    ('AI',      '人工智能',         'Artificial Intelligence',    'engineering'),
    ('MATH',    '数学',             'Mathematics',                'science'),
    ('PHYS',    '物理学',           'Physics',                    'science'),
    ('CHEM',    '化学',             'Chemistry',                  'science'),
    ('BIOL',    '生物学',           'Biology',                    'science'),
    ('MED',     '临床医学',         'Clinical Medicine',          'medicine'),
    ('ECON',    '经济学',           'Economics',                  'social_science'),
    ('MGMT',    '管理学',           'Management',                 'social_science'),
    ('HIST',    '历史学',           'History',                    'humanities'),
    ('LIT',     '文学',             'Literature',                 'humanities'),
    ('PHIL',    '哲学',             'Philosophy',                 'humanities'),
    ('LAW',     '法学',             'Law',                        'social_science'),
    ('EE',      '电子工程',         'Electrical Engineering',     'engineering'),
    ('MECH',    '机械工程',         'Mechanical Engineering',     'engineering'),
    ('CIVIL',   '土木工程',         'Civil Engineering',         'engineering'),
    ('MATSCI',  '材料科学',         'Materials Science',          'engineering'),
    ('PSYCH',   '心理学',           'Psychology',                 'social_science'),
    ('EDU',     '教育学',           'Education',                  'social_science');

-- 学院(初版 20 个常见学院)
INSERT INTO colleges (code, name_zh, name_en) VALUES
    ('CS',     '计算机学院',         'School of Computer Science'),
    ('SE',     '软件学院',           'School of Software'),
    ('AI',     '人工智能学院',       'School of Artificial Intelligence'),
    ('MATH',   '数学学院',           'School of Mathematics'),
    ('PHYS',   '物理学院',           'School of Physics'),
    ('CHEM',   '化学学院',           'School of Chemistry'),
    ('BIOL',   '生命科学学院',       'School of Life Sciences'),
    ('MED',    '医学院',             'School of Medicine'),
    ('ECON',   '经济学院',           'School of Economics'),
    ('MGMT',   '管理学院',           'School of Management'),
    ('HIST',   '历史学院',           'School of History'),
    ('LIT',    '文学院',             'School of Literature'),
    ('PHIL',   '哲学学院',           'School of Philosophy'),
    ('LAW',    '法学院',             'School of Law'),
    ('EE',     '电子工程学院',       'School of Electrical Engineering'),
    ('MECH',   '机械工程学院',       'School of Mechanical Engineering'),
    ('CIVIL',  '土木工程学院',       'School of Civil Engineering'),
    ('MATSCI', '材料科学与工程学院', 'School of Materials Science'),
    ('PSYCH',  '心理与认知科学学院', 'School of Psychology'),
    ('EDU',    '教育学院',           'School of Education');

-- 评分维度参考表(M3+ 用,先建表)
CREATE TABLE rating_dimensions (
    code TEXT PRIMARY KEY,
    name_zh TEXT NOT NULL,
    name_en TEXT NOT NULL,
    description_zh TEXT,
    description_en TEXT,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO rating_dimensions (code, name_zh, name_en, description_zh, description_en, sort_order) VALUES
    ('research', '科研能力',
     'Research Capability',
     '论文/项目/产出的整体水平',
     'Overall level of papers, projects, and outputs',
     1),
    ('resource', '资源调度能力',
     'Resource Allocation',
     '经费/实验室/合作网络/学生名额',
     'Funding, lab, collaboration network, student slots',
     2),
    ('fit',      '学科适配性',
     'Subject Fit',
     '导师方向与学生兴趣的契合度',
     'Alignment between supervisor direction and student interest',
     3),
    ('currency', '领域跟进度',
     'Domain Currency',
     '对前沿/新方法的把握',
     'Grasp of cutting-edge methods',
     4),
    ('ethic',    '行为正当性',
     'Ethical Conduct',
     '署名/引文/师生关系/利益冲突',
     'Authorship, citations, mentor-student relations, conflicts of interest',
     5),
    ('tool',     '新兴工具支持率',
     'Emerging Tool Support',
     '对学生使用 AI/新工具的态度',
     'Attitude toward students using AI/new tools',
     6);

COMMENT ON TABLE disciplines IS 'Lookup table: disciplines (user-declared)';
COMMENT ON TABLE colleges IS 'Lookup table: colleges (user-declared + supervisor classification)';
COMMENT ON TABLE rating_dimensions IS 'Lookup table: 6 rating dimensions (see §3)';
