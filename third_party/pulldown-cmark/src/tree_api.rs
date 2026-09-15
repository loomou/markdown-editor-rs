//! 解析完成的文档树:`Parsed` / `NodeRef` / `NodeKind`(feature `tree`)。
//!
//! 与 [`Parser`](crate::Parser) 的关系:`Parser` 把内部树拆成一次性事件
//! 流,取走内部分配的字符串;`Parsed` 在构造时把全部行内解析跑完,之后
//! 只借出引用,同一棵树可重复遍历。这是「下游不再逐叶重解析」的前提。
//!
//! 两条路并行,不互转:`Parsed` 不实现迭代器,`Parser` 不暴露树。行内
//! 节点的 span 与 `OffsetIter` 给出的 Range 相同(块级节点的 span 含
//! 容器续行前缀,与事件层一致)。
//!
//! 该模块不引入新的数据结构:遍历走 `Tree<Item>` 的 `child` / `next`
//! 链,内容读取走 `Allocations` 的 `Index` 实现,不复制树、不建 parent
//! 数组。

use crate::parse::{eager_parse, Allocations, BrokenLinkCallback, Item, ItemBody, RefDefs};
use crate::strings::CowStr;
use crate::tree::{Tree, TreeIndex};
use crate::{Alignment, BlockQuoteKind, HeadingLevel, LinkType, MetadataBlockKind, Options};
use core::ops::Range;

/// 解析完成的文档树。行内已全部 resolve,可重复只读遍历。
///
/// 与 [`Parser`](crate::Parser) 的区别:`Parser` 是一次性事件流,取走
/// 内部分配的字符串;`Parsed` 持有整棵树并只借出引用。代价是构造时就
/// 跑完全部行内解析,不像 `Parser` 那样按需惰性处理。
pub struct Parsed<'input> {
    text: &'input str,
    tree: Tree<Item>,
    allocs: Allocations<'input>,
}

impl core::fmt::Debug for Parsed<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Parsed")
            .field("text_len", &self.text.len())
            .field("node_count", &self.node_count())
            .finish()
    }
}

impl<'input> Parsed<'input> {
    /// 解析整篇文档:块结构 + 全部行内。
    pub fn new(text: &'input str, options: Options) -> Self {
        Self::new_with_broken_link_callback(text, options, None::<crate::DefaultBrokenLinkCallback>)
    }

    /// 带断链回调的版本,与 `Parser::new_with_broken_link_callback` 对称。
    pub fn new_with_broken_link_callback<F: BrokenLinkCallback<'input>>(
        text: &'input str,
        options: Options,
        broken_link_callback: Option<F>,
    ) -> Self {
        let (tree, allocs) = eager_parse(text, options, broken_link_callback);
        Parsed { text, tree, allocs }
    }

    /// 只解析行内:把 `text` 当作单个段落的内容。
    ///
    /// 给增量编辑用:一个 phrasing 叶的源文重解析时,不需要重新判定块
    /// 结构(块结构由调用方的文档模型持有)。
    ///
    /// **实现口径(重要)**:本方法并不真正关闭块级语法(CommonMark 的
    /// 块规则不受 option 控制),它就是一次完整解析。调用方需要自己判定
    /// 「`text` 是否恰好构成单个段落」——
    /// `root().children().count() == 1` 且首个子节点是 `Paragraph` /
    /// `TightParagraph`;不满足时(`text` 里有 `# `、`- `、围栏等块级
    /// 语法)走「换块」分支。块语法在叶内出现的语义由调用方定义,不是
    /// 本 API 的保证。
    pub fn inline_only(text: &'input str, options: Options) -> Self {
        Self::new(text, options)
    }

    /// [`inline_only`](Self::inline_only) 的出口判定:结果是否为单个
    /// 段落。false 表示 `text` 含块级语法(`# `、`- `、围栏 …)或多个
    /// 根块,调用方应走「换块 / display = source」分支(`tree-api-07` §3
    /// 的 reconcile 表)。
    ///
    /// 空 `text`(root 无孩子)按 true 处理:零内容段落的投影就是空,
    /// 与「换块」分支的结果相同,不必让调用方多分一支。尾部只有引用定义
    /// 时也是 true——定义收进 `reference_definitions()`,不占 root 的
    /// 孩子(`with_definitions` 拼定义后重解析的场景,`tree-api-07` §4)。
    pub fn is_single_paragraph(&self) -> bool {
        let mut children = self.root().children();
        match children.next() {
            Some(node) => {
                matches!(node.kind(), NodeKind::Paragraph | NodeKind::TightParagraph)
                    && children.next().is_none()
            }
            None => true,
        }
    }

    /// 文档根。`NodeKind::Root`,其 children 是顶层块。
    pub fn root(&self) -> NodeRef<'_, 'input> {
        NodeRef {
            parsed: self,
            ix: None,
        }
    }

    /// 原始输入。
    pub fn text(&self) -> &'input str {
        self.text
    }

    /// 与 `Parser::reference_definitions` 同语义。
    pub fn reference_definitions(&self) -> &RefDefs<'input> {
        &self.allocs.refdefs
    }

    /// 脚注定义及其被引用次数(全部行内 resolve 之后是全篇准确值)。
    /// 按 label 排序,顺序确定。
    pub fn footnote_definitions(&self) -> impl Iterator<Item = (&str, usize)> + '_ {
        let mut defs: Vec<_> = self
            .allocs
            .footdefs
            .0
            .iter()
            .map(|(label, def)| (label.as_ref(), def.use_count))
            .collect();
        defs.sort_unstable_by(|a, b| a.0.cmp(b.0));
        defs.into_iter()
    }

    /// 节点总数(含 root、含在事件层隐形的 `TightParagraph`)。容量预估用。
    pub fn node_count(&self) -> usize {
        self.tree.nodes().len()
    }

    fn node(&self, ix: TreeIndex) -> &crate::tree::Node<Item> {
        &self.tree.nodes()[ix.get()]
    }
}

/// 树中一个节点的只读句柄。`Copy`,随手传。
///
/// 没有反查父节点的方法:上游树没有 parent 指针,补一个 parent 数组
/// 等于每节点多 8 字节;walk 时天然知道父亲。
#[derive(Copy, Clone, Debug)]
pub struct NodeRef<'a, 'input> {
    parsed: &'a Parsed<'input>,
    /// `None` = 根(树的下标 0 是 dummy,`TreeIndex` 是 NonZero,表达不了)。
    ix: Option<TreeIndex>,
}

impl<'a, 'input> NodeRef<'a, 'input> {
    // ── 结构 ──────────────────────────────────────────────
    pub fn kind(&self) -> NodeKind<'a, 'input> {
        let Some(ix) = self.ix else {
            return NodeKind::Root;
        };
        let item = self.parsed.node(ix).item;
        let allocs = &self.parsed.allocs;
        match item.body {
            ItemBody::Root => NodeKind::Root,
            ItemBody::Paragraph => NodeKind::Paragraph,
            ItemBody::TightParagraph => NodeKind::TightParagraph,
            ItemBody::Rule => NodeKind::Rule,
            ItemBody::Heading(level, Some(hx)) => {
                let attrs = &allocs[hx];
                NodeKind::Heading(HeadingInfo {
                    level,
                    id: attrs.id.as_ref(),
                    classes: &attrs.classes,
                    attrs: &attrs.attrs,
                    setext: is_setext_span(self.parsed.text, item.start..item.end),
                })
            }
            ItemBody::Heading(level, None) => NodeKind::Heading(HeadingInfo {
                level,
                id: None,
                classes: &[],
                attrs: &[],
                setext: is_setext_span(self.parsed.text, item.start..item.end),
            }),
            ItemBody::FencedCodeBlock(cx) => {
                let (marker, len) = fence_style(self.parsed.text, item.start..item.end);
                NodeKind::CodeBlock(CodeBlockInfo::Fenced {
                    info: &allocs[cx],
                    marker,
                    len,
                })
            }
            ItemBody::IndentCodeBlock => NodeKind::CodeBlock(CodeBlockInfo::Indented),
            ItemBody::HtmlBlock => NodeKind::HtmlBlock,
            ItemBody::BlockQuote(kind) => NodeKind::BlockQuote(kind),
            ItemBody::List(tight, marker, start) => NodeKind::List(ListInfo {
                tight,
                marker,
                start,
                ordered: marker == b'.' || marker == b')',
            }),
            ItemBody::ListItem(indent) => NodeKind::ListItem(ListItemInfo { indent }),
            ItemBody::FootnoteDefinition(cx) => NodeKind::FootnoteDefinition(&allocs[cx]),
            ItemBody::MetadataBlock(kind) => NodeKind::MetadataBlock(kind),
            ItemBody::DefinitionList(tight) => NodeKind::DefinitionList { tight },
            ItemBody::DefinitionListTitle => NodeKind::DefinitionListTitle,
            ItemBody::DefinitionListDefinition(indent) => {
                NodeKind::DefinitionListDefinition { indent }
            }
            ItemBody::Table(ax) => NodeKind::Table(&allocs[ax]),
            ItemBody::TableHead => NodeKind::TableHead,
            ItemBody::TableRow => NodeKind::TableRow,
            ItemBody::TableCell => NodeKind::TableCell,
            ItemBody::Emphasis => NodeKind::Emphasis,
            ItemBody::Strong => NodeKind::Strong,
            ItemBody::Strikethrough => NodeKind::Strikethrough,
            ItemBody::Superscript => NodeKind::Superscript,
            ItemBody::Subscript => NodeKind::Subscript,
            ItemBody::Link(lx) => {
                let (link_type, dest_url, title, id) = &allocs[lx];
                NodeKind::Link(LinkInfo {
                    link_type: *link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            ItemBody::Image(lx) => {
                let (link_type, dest_url, title, id) = &allocs[lx];
                NodeKind::Image(LinkInfo {
                    link_type: *link_type,
                    dest_url,
                    title,
                    id,
                })
            }
            ItemBody::Text { backslash_escaped } => NodeKind::Text { backslash_escaped },
            ItemBody::SynthesizeText(_) | ItemBody::OwnedInlineHtml(_) => NodeKind::TextOwned,
            ItemBody::SynthesizeChar(c) => NodeKind::SynthesizedChar(c),
            ItemBody::Code(cx) => NodeKind::Code(&allocs[cx]),
            ItemBody::Math(cx, display) => NodeKind::Math {
                content: &allocs[cx],
                display,
            },
            ItemBody::InlineHtml => NodeKind::InlineHtml,
            ItemBody::Html => NodeKind::Html,
            ItemBody::FootnoteReference(cx) => NodeKind::FootnoteReference(&allocs[cx]),
            ItemBody::TaskListMarker(checked) => NodeKind::TaskMarker { checked },
            ItemBody::SoftBreak => NodeKind::SoftBreak,
            ItemBody::HardBreak(backslash) => NodeKind::HardBreak { backslash },
            body => unreachable!("unresolved inline body in Parsed: {body:?}"),
        }
    }

    pub fn children(&self) -> Children<'a, 'input> {
        let next = match self.ix {
            None => self.parsed.tree.first_index(),
            Some(ix) => self.parsed.node(ix).child,
        };
        Children {
            parsed: self.parsed,
            next,
        }
    }

    pub fn first_child(&self) -> Option<Self> {
        self.children().next()
    }

    pub fn next_sibling(&self) -> Option<Self> {
        let ix = self.ix?;
        self.parsed.node(ix).next.map(|next| NodeRef {
            parsed: self.parsed,
            ix: Some(next),
        })
    }

    // ── 位置 ──────────────────────────────────────────────
    /// 源文字节范围。**块级节点含容器续行前缀**(`> `、列表缩进)。
    pub fn span(&self) -> Range<usize> {
        match self.ix {
            None => 0..self.parsed.text.len(),
            Some(ix) => {
                let item = self.parsed.node(ix).item;
                item.start..item.end
            }
        }
    }

    // ── 内容 ──────────────────────────────────────────────
    /// 文本节点的内容。借 input 的切片(`NodeKind::Text`)或借 arena
    /// 里的解码结果(`NodeKind::TextOwned`);非文本节点返回 `None`。
    pub fn text(&self) -> Option<&'a str> {
        let ix = self.ix?;
        let item = self.parsed.node(ix).item;
        match item.body {
            ItemBody::Text { .. } | ItemBody::Html | ItemBody::InlineHtml => {
                self.parsed.text.get(item.start..item.end)
            }
            ItemBody::SynthesizeText(cx) | ItemBody::OwnedInlineHtml(cx) => {
                Some(self.parsed.allocs[cx].as_ref())
            }
            _ => None,
        }
    }
}

/// `child` / `next` 链上的迭代器,零分配。
#[derive(Clone, Debug)]
pub struct Children<'a, 'input> {
    parsed: &'a Parsed<'input>,
    next: Option<TreeIndex>,
}

impl<'a, 'input> Iterator for Children<'a, 'input> {
    type Item = NodeRef<'a, 'input>;

    fn next(&mut self) -> Option<NodeRef<'a, 'input>> {
        let ix = self.next?;
        self.next = self.parsed.node(ix).next;
        Some(NodeRef {
            parsed: self.parsed,
            ix: Some(ix),
        })
    }
}

/// 节点分类。信息全部挂在枚举上,消费方一次 match 就能建自己的节点。
///
/// `Text` 与 `TextOwned` 分开是有意的:前者的 `span()` 与内容一一对应
/// (可直接当 source range 用),后者的 span 是源范围但内容是解码结果
/// (实体、smart quote、合成字符),长度与内容长度不等。
#[derive(Clone, Debug)]
pub enum NodeKind<'a, 'input> {
    Root,

    // ── 块级容器 ──────────────────────────────────────────
    BlockQuote(Option<BlockQuoteKind>),
    List(ListInfo),
    ListItem(ListItemInfo),
    FootnoteDefinition(&'a CowStr<'input>),
    Table(&'a [Alignment]),
    TableHead,
    TableRow,
    TableCell,
    /// `Options::ENABLE_DEFINITION_LIST`。不开该 option 时不会出现,
    /// 覆盖它避免消费方出现 unreachable。
    DefinitionList {
        tight: bool,
    },
    DefinitionListTitle,
    DefinitionListDefinition {
        indent: usize,
    },

    // ── 块级叶子 ──────────────────────────────────────────
    Paragraph,
    /// 紧列表项里的段落。事件流里完全不发,走树时是真实节点:
    /// **它的存在与否就是列表松紧判据**。
    TightParagraph,
    Heading(HeadingInfo<'a, 'input>),
    CodeBlock(CodeBlockInfo<'a, 'input>),
    HtmlBlock,
    MetadataBlock(MetadataBlockKind),
    Rule,

    // ── 行内容器 ──────────────────────────────────────────
    Emphasis,
    Strong,
    Strikethrough,
    Superscript,
    Subscript,
    Link(LinkInfo<'a, 'input>),
    Image(LinkInfo<'a, 'input>),

    // ── 行内叶子 ──────────────────────────────────────────
    /// 借 input 的切片。`span()` 与内容一一对应(可当 source_range)。
    Text {
        backslash_escaped: bool,
    },
    /// 解码后与源文不同的文本(实体、smart quote 产出的合成串)。
    /// `span()` 仍是源范围,但长度与内容长度不等。
    TextOwned,
    /// 单个合成字符(smart quote 等),内容不在 arena 里,以 `char` 给出。
    SynthesizedChar(char),
    Code(&'a CowStr<'input>),
    Math {
        content: &'a CowStr<'input>,
        display: bool,
    },
    /// 行内 HTML。内容走 [`NodeRef::text`](`OwnedInlineHtml` 的内容在
    /// arena 里,事件层同样发 `Event::InlineHtml`,归并到本变体)。
    InlineHtml,
    /// 行内 HTML 之外由 inline pass 产出的 HTML 文本段
    /// (事件层的 `Event::Html`)。
    Html,
    FootnoteReference(&'a CowStr<'input>),
    TaskMarker {
        checked: bool,
    },
    SoftBreak,
    HardBreak {
        backslash: bool,
    },
}

#[derive(Copy, Clone, Debug)]
pub struct ListInfo {
    /// 上游 `ItemBody::List.0`。**紧 = 项内容不包段落。**
    pub tight: bool,
    /// 标记字节:`-` `+` `*` `.` `)`。上游 `ItemBody::List.1`。
    pub marker: u8,
    /// 有序列表起始号;`marker` 是 `.` 或 `)` 时有意义。
    pub start: u64,
    /// `marker` 是 `.` 或 `)`。便利方法,等价于自己判字节。
    pub ordered: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct ListItemInfo {
    /// 上游 `ItemBody::ListItem.0`:内容相对本项起点的缩进列数。
    pub indent: usize,
}

#[derive(Clone, Debug)]
pub struct HeadingInfo<'a, 'input> {
    pub level: HeadingLevel,
    pub id: Option<&'a CowStr<'input>>,
    pub classes: &'a [CowStr<'input>],
    pub attrs: &'a [(CowStr<'input>, Option<CowStr<'input>>)],
    /// setext 形式(`span()` 含下划线行)。按「span 首个非空白字节不是
    /// `#`(或 `#` 后无空白/行尾)」判定,与 ATX 的规则一致。
    pub setext: bool,
}

#[derive(Clone, Debug)]
pub enum CodeBlockInfo<'a, 'input> {
    Indented,
    Fenced {
        info: &'a CowStr<'input>,
        /// 围栏字符:`` ` `` 或 `~`。从 `span()` 首行扫出,上游未存。
        marker: u8,
        /// 开围栏字符数(≥3)。
        len: u16,
    },
}

#[derive(Clone, Debug)]
pub struct LinkInfo<'a, 'input> {
    pub link_type: LinkType,
    pub dest_url: &'a CowStr<'input>,
    pub title: &'a CowStr<'input>,
    pub id: &'a CowStr<'input>,
}

/// 围栏字符与长度:扫 `span()` 首行。上游 `parse_fenced_code_block` 用完
/// 即弃,树里只有 info string,这里补扫一次。
fn fence_style(text: &str, span: Range<usize>) -> (u8, u16) {
    let line = text.get(span).unwrap_or("").lines().next().unwrap_or("");
    let line = line.trim_start_matches(' ');
    let marker = if line.starts_with('~') { b'~' } else { b'`' };
    let len = line.bytes().take_while(|b| *b == marker).count();
    (marker, len.clamp(3, u16::MAX as usize) as u16)
}

/// setext 判定:首个非空白字节不是 `#`,或是 `#` 但后面既非空白也非行尾
/// (那不是合法 ATX,正文被 setext 下划线行抬成标题)。
fn is_setext_span(text: &str, span: Range<usize>) -> bool {
    let line = text.get(span).unwrap_or("").lines().next().unwrap_or("");
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix('#') {
        rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
    } else {
        true
    }
}
