//! 아주 단순한 XML 트리 파서 (WebDAV/S3 응답용, 네임스페이스 접두사 무시)
use anyhow::Result;
use quick_xml::events::Event;
use quick_xml::Reader;

#[derive(Debug, Default, Clone)]
pub struct XmlNode {
    pub name: String,
    pub text: String,
    pub children: Vec<XmlNode>,
}

impl XmlNode {
    pub fn child(&self, name: &str) -> Option<&XmlNode> {
        self.children
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
    }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a XmlNode> {
        self.children
            .iter()
            .filter(move |c| c.name.eq_ignore_ascii_case(name))
    }
    /// 경로를 따라 내려가 텍스트 반환
    pub fn text_at(&self, path: &[&str]) -> Option<&str> {
        let mut cur = self;
        for p in path {
            cur = cur.child(p)?;
        }
        Some(cur.text.as_str())
    }
    #[allow(dead_code)]
    /// 하위 트리 어디에든 해당 이름의 노드가 있는지 (깊이 우선)
    pub fn find(&self, name: &str) -> Option<&XmlNode> {
        for c in &self.children {
            if c.name.eq_ignore_ascii_case(name) {
                return Some(c);
            }
            if let Some(f) = c.find(name) {
                return Some(f);
            }
        }
        None
    }
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    match s.rfind(':') {
        Some(i) => s[i + 1..].to_string(),
        None => s.to_string(),
    }
}

pub fn parse(s: &str) -> Result<XmlNode> {
    let mut reader = Reader::from_str(s);
    reader.config_mut().trim_text(true);
    let mut stack: Vec<XmlNode> = vec![XmlNode {
        name: "#root".into(),
        ..Default::default()
    }];
    loop {
        match reader.read_event()? {
            Event::Start(e) => stack.push(XmlNode {
                name: local(e.name().as_ref()),
                ..Default::default()
            }),
            Event::Empty(e) => {
                let node = XmlNode {
                    name: local(e.name().as_ref()),
                    ..Default::default()
                };
                stack.last_mut().unwrap().children.push(node);
            }
            Event::End(_) => {
                if stack.len() > 1 {
                    let node = stack.pop().unwrap();
                    stack.last_mut().unwrap().children.push(node);
                }
            }
            Event::Text(t) => {
                let txt = t.unescape()?.into_owned();
                stack.last_mut().unwrap().text.push_str(&txt);
            }
            Event::CData(t) => {
                let txt = String::from_utf8_lossy(&t.into_inner()).into_owned();
                stack.last_mut().unwrap().text.push_str(&txt);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    while stack.len() > 1 {
        let node = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(node);
    }
    Ok(stack.pop().unwrap())
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_multistatus() {
        let x = r#"<?xml version="1.0"?><D:multistatus xmlns:D="DAV:"><D:response><D:href>/a/b%20c.md</D:href><D:propstat><D:prop><D:getetag>"abc"</D:getetag><D:resourcetype/></D:prop></D:propstat></D:response></D:multistatus>"#;
        let root = super::parse(x).unwrap();
        let ms = root.child("multistatus").unwrap();
        let r = ms.children_named("response").next().unwrap();
        assert_eq!(r.text_at(&["href"]), Some("/a/b%20c.md"));
        assert_eq!(r.find("getetag").unwrap().text, "\"abc\"");
    }
}
