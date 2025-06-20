use std::collections::{HashMap, HashSet, VecDeque};

use crate::error::ParseError;
use crate::lexer::{Item, ItemType, Lexer};
use crate::node::*;
use crate::utils::*;

pub struct Parser {
    name: String,
    pub funcs: HashSet<String>,
    lex: Option<Lexer>,
    line: usize,
    token: VecDeque<Item>,
    peek_count: usize,
    pub tree_set: HashMap<String, Tree>,
    tree_id: TreeId,
    tree: Option<Tree>,
    tree_stack: VecDeque<Tree>,
    max_tree_id: TreeId,
}

#[derive(Clone)]
pub struct Tree {
    name: String,
    id: TreeId,
    pub root: Option<Nodes>,
    vars: Vec<String>,
    pub fields: HashSet<String>,
}

impl Parser {
    pub fn new(name: String) -> Parser {
        Parser {
            name,
            funcs: HashSet::new(),
            lex: None,
            line: 0,
            token: VecDeque::new(),
            peek_count: 0,
            tree_set: HashMap::new(),
            tree_id: 0,
            tree: None,
            tree_stack: VecDeque::new(),
            max_tree_id: 0,
        }
    }
}

impl Tree {
    fn new(name: String, id: TreeId) -> Tree {
        Tree {
            name,
            id,
            root: None,
            vars: vec![],
            fields: Default::default(),
        }
    }

    pub fn pop_vars(&mut self, n: usize) {
        self.vars.truncate(n);
    }
}

pub fn parse(
    name: String,
    text: String,
    funcs: HashSet<String>,
) -> Result<HashMap<String, Tree>, ParseError> {
    let mut p = Parser::new(name);
    p.funcs = funcs;
    p.lex = Some(Lexer::new(text));
    p.parse_tree()?;
    Ok(p.tree_set)
}

impl Parser {
    fn next_from_lex(&mut self) -> Option<Item> {
        match self.lex {
            Some(ref mut l) => l.next(),
            None => None,
        }
    }

    fn backup(&mut self, t: Item) {
        self.token.push_front(t);
        self.peek_count += 1;
    }

    fn backup2(&mut self, t0: Item, t1: Item) {
        self.token.push_front(t1);
        self.token.push_front(t0);
        self.peek_count += 2;
    }

    fn backup3(&mut self, t0: Item, t1: Item, t2: Item) {
        self.token.push_front(t2);
        self.token.push_front(t1);
        self.token.push_front(t0);
        self.peek_count += 3;
    }

    fn next_must(&mut self, context: &str) -> Result<Item, ParseError> {
        self.next()
            .ok_or_else(|| self.error_msg(&format!("unexpected end in {}", context)))
    }

    fn next_non_space(&mut self) -> Option<Item> {
        self.find(|c| c.typ != ItemType::ItemSpace)
    }

    fn next_non_space_must(&mut self, context: &str) -> Result<Item, ParseError> {
        self.next_non_space()
            .ok_or_else(|| self.unexpected("end", context))
    }

    fn peek_non_space_must(&mut self, context: &str) -> Result<&Item, ParseError> {
        if let Some(t) = self.next_non_space() {
            self.backup(t);
            return Ok(self.token.front().unwrap());
        }
        self.error(&format!("unexpected end in {}", context))
    }

    fn peek(&mut self) -> Option<&Item> {
        if let Some(t) = self.next() {
            self.backup(t);
            return self.token.front();
        }
        None
    }

    fn peek_must(&mut self, context: &str) -> Result<&Item, ParseError> {
        if let Some(t) = self.next_non_space() {
            self.backup(t);
            return Ok(self.token.front().unwrap());
        }
        self.error(&format!("unexpected end in {}", context))
    }

    fn start_parse(&mut self, name: String, id: TreeId) {
        if let Some(t) = self.tree.take() {
            self.tree_stack.push_back(t);
        }
        self.tree_id = id;
        let t = Tree::new(name, id);
        self.tree = Some(t);
    }

    fn stop_parse(&mut self) -> Result<(), ParseError> {
        self.add_to_tree_set()?;
        self.tree = self.tree_stack.pop_back();
        self.tree_id = self.tree.as_ref().map(|t| t.id).unwrap_or(0);
        Ok(())
    }

    // top level parser
    fn parse_tree(&mut self) -> Result<(), ParseError> {
        let name = self.name.clone();
        self.start_parse(name, 1);
        self.parse()?;
        self.stop_parse()?;
        Ok(())
    }

    fn add_tree(&mut self, name: String, t: Tree) {
        self.tree_set.insert(name, t);
    }

    fn error<T>(&self, msg: &str) -> Result<T, ParseError> {
        Err(self.error_msg(msg))
    }

    fn error_msg(&self, msg: &str) -> ParseError {
        let name = if let Some(t) = self.tree.as_ref() {
            &t.name
        } else {
            &self.name
        };
        ParseError::with_context(name, self.line, msg)
    }

    fn expect(&mut self, expected: &ItemType, context: &str) -> Result<Item, ParseError> {
        let token = self.next_non_space_must(context)?;
        if token.typ != *expected {
            return Err(self.unexpected(&token, context));
        }
        Ok(token)
    }

    fn unexpected(
        &self,
        token: impl std::fmt::Display,
        context: impl std::fmt::Display,
    ) -> ParseError {
        self.error_msg(&format!("unexpected {} in {}", token, context))
    }

    fn add_var(&mut self, name: String) -> Result<(), ParseError> {
        let mut tree = self.tree.take().ok_or_else(|| self.error_msg("no tree"))?;
        tree.vars.push(name);
        self.tree = Some(tree);
        Ok(())
    }

    fn add_to_tree_set(&mut self) -> Result<(), ParseError> {
        let tree = self.tree.take().ok_or_else(|| self.error_msg("no tree"))?;
        if let Some(t) = self.tree_set.get(tree.name.as_str()) {
            if let Some(ref r) = t.root {
                match r.is_empty_tree() {
                    Err(e) => return Err(e.into()),
                    Ok(false) => {
                        let err =
                            format!("template multiple definitions of template {}", &tree.name);
                        return self.error(&err);
                    }
                    Ok(true) => {}
                }
            }
        }
        self.add_tree(tree.name.clone(), tree);
        Ok(())
    }

    fn has_func(&self, name: &str) -> bool {
        self.funcs.contains(name)
    }

    fn parse(&mut self) -> Result<(), ParseError> {
        if self.tree.is_none() {
            return self.error("no tree");
        }
        let id = self.tree_id;
        let mut t = match self.next() {
            None => return self.error(&format!("unable to peek for tree {}", id)),
            Some(t) => t,
        };
        if let Some(tree) = self.tree.as_mut() {
            tree.root = Some(Nodes::List(ListNode::new(id, t.pos)));
        }
        while t.typ != ItemType::ItemEOF {
            if t.typ == ItemType::ItemLeftDelim {
                let nns = self.next_non_space();
                match nns {
                    Some(ref item) if item.typ == ItemType::ItemDefine => {
                        self.parse_definition()?;
                        t = match self.next() {
                            None => return self.error(&format!("unable to peek for tree {}", id)),
                            Some(t) => t,
                        };
                        continue;
                    }
                    _ => {}
                };
                if let Some(t2) = nns {
                    self.backup2(t, t2);
                } else {
                    self.backup(t);
                }
            } else {
                self.backup(t);
            }
            let node = match self.text_or_action() {
                Ok(Nodes::Else(node)) => return self.error(&format!("unexpected {}", node)),
                Ok(Nodes::End(node)) => return self.error(&format!("unexpected {}", node)),
                Ok(node) => node,
                Err(e) => return Err(e),
            };
            self.tree
                .as_mut()
                .and_then(|tree| {
                    tree.root.as_mut().and_then(|r| match *r {
                        Nodes::List(ref mut r) => {
                            r.append(node);
                            Some(())
                        }
                        _ => None,
                    })
                })
                .ok_or_else(|| self.error_msg("invalid root node"))?;

            t = match self.next() {
                None => return self.error(&format!("unable to peek for tree {}", id)),
                Some(t) => t,
            };
        }
        self.backup(t);
        Ok(())
    }

    fn parse_definition(&mut self) -> Result<(), ParseError> {
        let context = "define clause";
        let id = self.tree_id;
        let token = self.next_non_space_must(context)?;
        let name = self.parse_template_name(&token, context)?;
        self.expect(&ItemType::ItemRightDelim, "define end")?;
        self.start_parse(name, id + 1);
        let (list, end) = self.item_list()?;
        if *end.typ() != NodeType::End {
            return Err(self.unexpected(&end, context));
        }
        if let Some(tree) = self.tree.as_mut() {
            tree.root = Some(Nodes::List(list));
        }
        self.stop_parse()
    }

    fn item_list(&mut self) -> Result<(ListNode, Nodes), ParseError> {
        let pos = self.peek_non_space_must("item list")?.pos;
        let mut list = ListNode::new(self.tree_id, pos);
        while self.peek_non_space_must("item list")?.typ != ItemType::ItemEOF {
            let node = self.text_or_action()?;
            match *node.typ() {
                NodeType::End | NodeType::Else => return Ok((list, node)),
                _ => list.append(node),
            }
        }
        self.error("unexpected EOF")
    }

    fn text_or_action(&mut self) -> Result<Nodes, ParseError> {
        match self.next_non_space() {
            Some(ref item) if item.typ == ItemType::ItemText => Ok(Nodes::Text(TextNode::new(
                self.tree_id,
                item.pos,
                item.val.clone(),
            ))),
            Some(ref item) if item.typ == ItemType::ItemLeftDelim => self.action(),
            Some(ref item) => Err(self.unexpected(item, "input")),
            _ => self.error("unexpected end of input"),
        }
    }

    fn action(&mut self) -> Result<Nodes, ParseError> {
        let token = self.next_non_space_must("action")?;
        match token.typ {
            ItemType::ItemBlock => return self.block_control(),
            ItemType::ItemElse => return self.else_control(),
            ItemType::ItemEnd => return self.end_control(),
            ItemType::ItemIf => return self.if_control(),
            ItemType::ItemRange => return self.range_control(),
            ItemType::ItemTemplate => return self.template_control(),
            ItemType::ItemWith => return self.with_control(),
            _ => {}
        }
        let pos = token.pos;
        self.backup(token);
        Ok(Nodes::Action(ActionNode::new(
            self.tree_id,
            pos,
            self.pipeline("command")?,
        )))
    }

    fn parse_control(
        &mut self,
        allow_else_if: bool,
        context: &str,
    ) -> Result<(Pos, PipeNode, ListNode, Option<ListNode>), ParseError> {
        let vars_len = self
            .tree
            .as_ref()
            .map(|t| t.vars.len())
            .ok_or(ParseError::NoTree)?;
        let pipe = self.pipeline(context)?;
        let (list, next) = self.item_list()?;
        let else_list = match *next.typ() {
            NodeType::End => None,
            NodeType::Else => {
                if allow_else_if && self.peek_must("else if")?.typ == ItemType::ItemIf {
                    self.next_must("else if")?;
                    let mut else_list = ListNode::new(self.tree_id, next.pos());
                    else_list.append(self.if_control()?);
                    Some(else_list)
                } else {
                    let (else_list, next) = self.item_list()?;
                    if *next.typ() != NodeType::End {
                        return self.error(&format!("expected end; found {}", next));
                    }
                    Some(else_list)
                }
            }
            _ => return self.error(&format!("expected end; found {}", next)),
        };
        if let Some(t) = self.tree.as_mut() {
            t.pop_vars(vars_len);
        }
        Ok((pipe.pos(), pipe, list, else_list))
    }

    fn if_control(&mut self) -> Result<Nodes, ParseError> {
        let (pos, pipe, list, else_list) = self.parse_control(true, "if")?;
        Ok(Nodes::If(IfNode::new_if(
            self.tree_id,
            pos,
            pipe,
            list,
            else_list,
        )))
    }

    fn range_control(&mut self) -> Result<Nodes, ParseError> {
        let (pos, pipe, list, else_list) = self.parse_control(false, "range")?;
        Ok(Nodes::Range(RangeNode::new_range(
            self.tree_id,
            pos,
            pipe,
            list,
            else_list,
        )))
    }

    fn with_control(&mut self) -> Result<Nodes, ParseError> {
        let (pos, pipe, list, else_list) = self.parse_control(false, "with")?;
        Ok(Nodes::With(WithNode::new_with(
            self.tree_id,
            pos,
            pipe,
            list,
            else_list,
        )))
    }

    fn end_control(&mut self) -> Result<Nodes, ParseError> {
        Ok(Nodes::End(EndNode::new(
            self.tree_id,
            self.expect(&ItemType::ItemRightDelim, "end")?.pos,
        )))
    }

    fn else_control(&mut self) -> Result<Nodes, ParseError> {
        if self.peek_non_space_must("else")?.typ == ItemType::ItemIf {
            let peek = self.peek_non_space_must("else")?;
            return Ok(Nodes::Else(ElseNode::new(peek.pos, peek.line)));
        }
        let token = self.expect(&ItemType::ItemRightDelim, "else")?;
        Ok(Nodes::Else(ElseNode::new(token.pos, token.line)))
    }

    fn block_control(&mut self) -> Result<Nodes, ParseError> {
        let context = "block clause";
        let token = self.next_non_space_must(context)?;
        let name = self.parse_template_name(&token, context)?;
        let pipe = self.pipeline(context)?;

        self.max_tree_id += 1;
        let tree_id = self.max_tree_id;
        self.start_parse(name.clone(), tree_id);
        let (root, end) = self.item_list()?;
        if let Some(tree) = self.tree.as_mut() {
            tree.root = Some(Nodes::List(root));
        }
        if end.typ() != &NodeType::End {
            return self.error(&format!("unexpected {} in {}", end, context));
        }
        self.stop_parse()?;
        Ok(Nodes::Template(TemplateNode::new(
            self.tree_id,
            token.pos,
            PipeOrString::String(name),
            Some(pipe),
        )))
    }

    fn template_control(&mut self) -> Result<Nodes, ParseError> {
        let context = "template clause";
        let token = self.next_non_space().ok_or(ParseError::UnexpectedEnd)?;
        let name = if let ItemType::ItemLeftParen = token.typ {
            #[cfg(feature = "gtmpl_dynamic_template")]
            {
                let pipe = self.pipeline(context)?;
                self.next_must("template name pipeline end")?;
                PipeOrString::Pipe(pipe)
            }
            #[cfg(not(feature = "gtmpl_dynamic_template"))]
            return Err(ParseError::NoDynamicTemplate.into());
        } else {
            PipeOrString::String(self.parse_template_name(&token, context)?)
        };
        let next = self.next_non_space().ok_or(ParseError::UnexpectedEnd)?;
        let pipe = if next.typ != ItemType::ItemRightDelim {
            self.backup(next);
            Some(self.pipeline(context)?)
        } else {
            None
        };
        Ok(Nodes::Template(TemplateNode::new(
            self.tree_id,
            token.pos,
            name,
            pipe,
        )))
    }

    fn pipeline(&mut self, context: &str) -> Result<PipeNode, ParseError> {
        let mut decl = vec![];
        let mut token = self.next_non_space_must("pipeline")?;
        let pos = token.pos;
        // TODO: test this hard!
        if token.typ == ItemType::ItemVariable {
            while token.typ == ItemType::ItemVariable {
                let token_after_var = self.next_must("variable")?;
                let next = if token_after_var.typ == ItemType::ItemSpace {
                    let next = self.next_non_space_must("variable")?;
                    if next.typ != ItemType::ItemColonEquals
                        && !(next.typ == ItemType::ItemChar && next.val == ",")
                    {
                        self.backup3(token, token_after_var, next);
                        break;
                    }
                    next
                } else {
                    token_after_var
                };
                if next.typ == ItemType::ItemColonEquals
                    || (next.typ == ItemType::ItemChar && next.val == ",")
                {
                    let variable = VariableNode::new(self.tree_id, token.pos, &token.val);
                    self.add_var(token.val.clone())?;
                    decl.push(variable);
                    if next.typ == ItemType::ItemChar && next.val == "," {
                        if context == "range" && decl.len() < 2 {
                            token = self.next_non_space_must("variable")?;
                            continue;
                        }
                        return self.error(&format!("to many decalarations in {}", context));
                    }
                } else {
                    self.backup2(token, next);
                }
                break;
            }
        } else {
            self.backup(token);
        }
        let mut pipe = PipeNode::new(self.tree_id, pos, decl);
        let mut token = self.next_non_space_must("pipeline")?;
        loop {
            match token.typ {
                ItemType::ItemRightDelim | ItemType::ItemRightParen => {
                    self.check_pipeline(&mut pipe, context)?;
                    if token.typ == ItemType::ItemRightParen {
                        self.backup(token);
                    }
                    return Ok(pipe);
                }
                ItemType::ItemBool
                | ItemType::ItemCharConstant
                | ItemType::ItemDot
                | ItemType::ItemField
                | ItemType::ItemIdentifier
                | ItemType::ItemNumber
                | ItemType::ItemNil
                | ItemType::ItemRawString
                | ItemType::ItemString
                | ItemType::ItemVariable
                | ItemType::ItemLeftParen => {
                    self.backup(token);
                    pipe.append(self.command()?);
                }
                _ => return Err(self.unexpected(&token, context)),
            }
            token = self.next_non_space_must("pipeline")?;
        }
    }

    fn check_pipeline(&mut self, pipe: &mut PipeNode, context: &str) -> Result<(), ParseError> {
        if pipe.cmds.is_empty() {
            return self.error(&format!("missing value for {}", context));
        }
        for (i, c) in pipe.cmds.iter().enumerate().skip(1) {
            match c.args.first() {
                Some(n) => match *n.typ() {
                    NodeType::Bool
                    | NodeType::Dot
                    | NodeType::Nil
                    | NodeType::Number
                    | NodeType::String => {
                        return self.error(&format!(
                            "non executable command in pipeline stage {}",
                            i + 2
                        ))
                    }
                    _ => {}
                },
                None => {
                    return self.error(&format!(
                        "non executable command in pipeline stage {}",
                        i + 2
                    ))
                }
            }
        }
        Ok(())
    }

    fn command(&mut self) -> Result<CommandNode, ParseError> {
        let mut cmd = CommandNode::new(self.tree_id, self.peek_non_space_must("command")?.pos);
        loop {
            self.peek_non_space_must("operand")?;
            if let Some(operand) = self.operand()? {
                cmd.append(operand);
            }
            let token = self.next_must("command")?;
            match token.typ {
                ItemType::ItemSpace => continue,
                ItemType::ItemError => return self.error(&token.val),
                ItemType::ItemRightDelim | ItemType::ItemRightParen => self.backup(token),
                ItemType::ItemPipe => {}
                _ => return self.error(&format!("unexpected {} in operand", token)),
            };
            break;
        }
        if cmd.args.is_empty() {
            return self.error("empty command");
        }
        Ok(cmd)
    }

    fn operand(&mut self) -> Result<Option<Nodes>, ParseError> {
        let node = self.term()?;
        match node {
            None => Ok(None),
            Some(n) => {
                let next = self.next_must("operand")?;
                if next.typ == ItemType::ItemField {
                    let typ = n.typ().clone();
                    match typ {
                        NodeType::Bool
                        | NodeType::String
                        | NodeType::Number
                        | NodeType::Nil
                        | NodeType::Dot => {
                            return self
                                .error(&format!("unexpected . after term {}", n.to_string()));
                        }
                        _ => {}
                    };
                    let mut chain = ChainNode::new(self.tree_id, next.pos, n);
                    chain.add(&next.val);
                    while self
                        .peek()
                        .map(|p| p.typ == ItemType::ItemField)
                        .unwrap_or(false)
                    {
                        let field = self.next().unwrap();
                        chain.add(&field.val);
                    }
                    let n = match typ {
                        NodeType::Field => {
                            let field = chain.to_string();
                            self.tree.as_mut().unwrap().fields.insert(field.clone());
                            Nodes::Field(FieldNode::new(self.tree_id, chain.pos(), &field))
                        }
                        NodeType::Variable => Nodes::Variable(VariableNode::new(
                            self.tree_id,
                            chain.pos(),
                            &chain.to_string(),
                        )),
                        _ => Nodes::Chain(chain),
                    };
                    Ok(Some(n))
                } else {
                    self.backup(next);
                    Ok(Some(n))
                }
            }
        }
    }

    fn term(&mut self) -> Result<Option<Nodes>, ParseError> {
        let token = self.next_non_space_must("token")?;
        let node = match token.typ {
            ItemType::ItemError => return self.error(&token.val),
            ItemType::ItemIdentifier => {
                if !self.has_func(&token.val) {
                    return self.error(&format!("function {} not defined", token.val));
                }
                let mut node = IdentifierNode::new(token.val);
                node.set_pos(token.pos);
                node.set_tree(self.tree_id);
                Nodes::Identifier(node)
            }
            ItemType::ItemDot => Nodes::Dot(DotNode::new(self.tree_id, token.pos)),
            ItemType::ItemNil => Nodes::Nil(NilNode::new(self.tree_id, token.pos)),
            ItemType::ItemVariable => {
                Nodes::Variable(self.use_var(self.tree_id, token.pos, &token.val)?)
            }
            ItemType::ItemField => {
                let field = &token.val;
                self.tree.as_mut().unwrap().fields.insert(field.clone());
                Nodes::Field(FieldNode::new(self.tree_id, token.pos, &field))
            }
            ItemType::ItemBool => {
                Nodes::Bool(BoolNode::new(self.tree_id, token.pos, token.val == "true"))
            }
            ItemType::ItemCharConstant | ItemType::ItemNumber => {
                match NumberNode::new(self.tree_id, token.pos, token.val, &token.typ) {
                    Ok(n) => Nodes::Number(n),
                    Err(e) => return self.error(&e.to_string()),
                }
            }
            ItemType::ItemLeftParen => {
                let pipe = self.pipeline("parenthesized pipeline")?;
                let next = self.next_must("parenthesized pipeline")?;
                if next.typ != ItemType::ItemRightParen {
                    return self.error(&format!("unclosed right paren: unexpected {}", next));
                }
                Nodes::Pipe(pipe)
            }
            ItemType::ItemString | ItemType::ItemRawString => {
                if let Some(s) = unquote_str(&token.val) {
                    Nodes::String(StringNode::new(self.tree_id, token.pos, token.val, s))
                } else {
                    return self.error(&format!("unable to unqote string: {}", token.val));
                }
            }

            _ => {
                self.backup(token);
                return Ok(None);
            }
        };
        Ok(Some(node))
    }

    fn use_var(&self, tree_id: TreeId, pos: Pos, name: &str) -> Result<VariableNode, ParseError> {
        if name == "$" {
            return Ok(VariableNode::new(tree_id, pos, name));
        }
        self.tree
            .as_ref()
            .and_then(|t| {
                t.vars
                    .iter()
                    .find(|&v| v == name)
                    .map(|_| VariableNode::new(tree_id, pos, name))
            })
            .ok_or_else(|| self.error_msg(&format!("undefined variable {}", name)))
    }

    fn parse_template_name(&self, token: &Item, context: &str) -> Result<String, ParseError> {
        match token.typ {
            ItemType::ItemString | ItemType::ItemRawString => unquote_str(&token.val)
                .ok_or_else(|| ParseError::UnableToParseString(token.val.clone())),
            _ => Err(self.unexpected(token, context)),
        }
    }
}

impl Iterator for Parser {
    type Item = Item;
    fn next(&mut self) -> Option<Item> {
        let item = if self.peek_count > 0 {
            self.peek_count -= 1;
            self.token.pop_front()
        } else {
            self.next_from_lex()
        };
        match item {
            Some(item) => {
                self.line = item.line;
                Some(item)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests_mocked {
    use super::*;
    use crate::lexer::ItemType;
    use gtmpl_value::Value;

    /*
       ItemText
       ItemLeftDelim
       ItemSpace
       ItemIf
       ItemSpace
       ItemIdentifier
       ItemSpace
       ItemString
       ItemSpace
       ItemString
       ItemSpace
       ItemRightDelim
       ItemEOF
    */

    fn make_parser() -> Parser {
        let s = r#"something {{ if eq "foo" "bar" }}"#;
        make_parser_with(s)
    }

    fn make_parser_with(s: &str) -> Parser {
        make_parser_with_funcs(s, &[])
    }

    fn make_parser_with_funcs<'a>(s: &str, funcs: &[&'a str]) -> Parser {
        let lex = Lexer::new(s.to_owned());
        Parser {
            name: String::from("foo"),
            funcs: funcs.iter().map(|&k| k.to_owned()).collect(),
            lex: Some(lex),
            line: 0,
            token: VecDeque::new(),
            peek_count: 0,
            tree_set: HashMap::new(),
            tree_id: 0,
            tree: None,
            tree_stack: VecDeque::new(),
            max_tree_id: 0,
        }
    }

    #[test]
    fn test_iter() {
        let mut p = make_parser();
        assert_eq!(p.next().map(|n| n.typ), Some(ItemType::ItemText));
        assert_eq!(p.count(), 12);
    }

    #[test]
    fn test_backup() {
        let mut t = make_parser();
        assert_eq!(t.next().map(|n| n.typ), Some(ItemType::ItemText));
        let i = t.next().unwrap();
        let s = i.to_string();
        t.backup(i);
        assert_eq!(t.next().map(|n| n.to_string()), Some(s));
        assert_eq!(t.last().map(|n| n.typ), Some(ItemType::ItemEOF));
    }
    #[test]
    fn test_backup3() {
        let mut t = make_parser();
        assert_eq!(t.next().map(|n| n.typ), Some(ItemType::ItemText));
        let t0 = t.next().unwrap();
        let t1 = t.next().unwrap();
        let t2 = t.next().unwrap();
        assert_eq!(t0.typ, ItemType::ItemLeftDelim);
        assert_eq!(t1.typ, ItemType::ItemSpace);
        assert_eq!(t2.typ, ItemType::ItemIf);
        t.backup3(t0, t1, t2);
        let t0 = t.next().unwrap();
        let t1 = t.next().unwrap();
        let t2 = t.next().unwrap();
        assert_eq!(t0.typ, ItemType::ItemLeftDelim);
        assert_eq!(t1.typ, ItemType::ItemSpace);
        assert_eq!(t2.typ, ItemType::ItemIf);
        assert_eq!(t.last().map(|n| n.typ), Some(ItemType::ItemEOF));
    }

    #[test]
    fn test_next_non_space() {
        let mut t = make_parser();
        t.next();
        let i = t.next().unwrap();
        let typ = i.typ;
        assert_eq!(typ, ItemType::ItemLeftDelim);
        assert_eq!(t.next_non_space().map(|n| n.typ), Some(ItemType::ItemIf));
        assert_eq!(t.last().map(|n| n.typ), Some(ItemType::ItemEOF));
    }

    #[test]
    fn test_peek_non_space() {
        let mut t = make_parser();
        t.next();
        let i = t.next().unwrap();
        let typ = i.typ;
        assert_eq!(typ, ItemType::ItemLeftDelim);
        assert_eq!(
            t.peek_non_space_must("").map(|n| &n.typ).unwrap(),
            &ItemType::ItemIf
        );
        assert_eq!(t.next().map(|n| n.typ), Some(ItemType::ItemIf));
        assert_eq!(t.last().map(|n| n.typ), Some(ItemType::ItemEOF));
    }

    #[test]
    fn test_display() {
        let raw = r#"{{if .}}2000{{else}} 3000 {{end}}"#;
        let mut ts = parse(String::default(), String::from(raw), HashSet::default()).unwrap();
        let tree = ts.get_mut("").unwrap();
        if let Some(ref root) = tree.root {
            assert_eq!(raw, format!("{}", root))
        } else {
            panic!()
        }
    }

    #[test]
    fn parse_basic_tree() {
        let mut p = make_parser_with(r#"{{ if eq .foo "bar" }} 2000 {{ end }}"#);
        let r = p.parse_tree();
        assert_eq!(
            r.err().unwrap().to_string(),
            "template: foo:2:function eq not defined"
        );
        let funcs = &["eq"];
        let mut p = make_parser_with_funcs(r#"{{ if eq .foo "bar" }} 2000 {{ end }}"#, funcs);
        let r = p.parse_tree();
        assert!(r.is_ok());
        let funcs = &["eq"];
        let mut p = make_parser_with_funcs(r#"{{ if eq 1 2 }} 2000 {{ end }}"#, funcs);
        let r = p.parse_tree();
        assert!(r.is_ok());
    }

    #[test]
    fn test_pipeline_simple() {
        let mut p = make_parser_with(r#" $foo, $bar := yay | blub "2000" }}"#);
        let pipe = p.pipeline("range");
        // broken for now
        assert!(pipe.is_err());
    }

    #[test]
    fn test_assign_string() {
        let mut p = make_parser_with(r#"{{ with $bar := "foo" }}{{ $bar }}{{ end }}"#);
        let r = p.parse_tree();
        assert!(r.is_ok());
    }

    #[test]
    fn test_term() {
        let mut p = make_parser_with(r#"{{true}}"#);
        p.next();
        let t = p.term();
        // broken for now
        assert!(t.is_ok());
        let t = t.unwrap();
        assert!(t.is_some());
        let t = t.unwrap();
        assert_eq!(t.typ(), &NodeType::Bool);
        if let Nodes::Bool(n) = t {
            assert_eq!(n.value, Value::from(true));
        } else {
            panic!()
        }

        let mut p = make_parser_with(r#"{{ false }}"#);
        p.next();
        let t = p.term();
        // broken for now
        assert!(t.is_ok());
        let t = t.unwrap();
        assert!(t.is_some());
        let t = t.unwrap();
        assert_eq!(t.typ(), &NodeType::Bool);
        assert_eq!(t.typ(), &NodeType::Bool);
        if let Nodes::Bool(n) = t {
            assert_eq!(n.value, Value::from(false));
        } else {
            panic!()
        }
    }
}
