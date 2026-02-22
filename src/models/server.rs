use dioxus::prelude::*;
use liquid::ParserBuilder;
use liquid::{Error, Object};

use crate::models::{Device, Template};

pub trait TemplateAble {
    fn get_content(&self) -> &str;

    fn render(&self, globals: Object) -> Result<String, Error> {
        let parser = ParserBuilder::with_stdlib().build()?;

        let template = parser.parse(self.get_content())?;

        Ok(template.render(&globals)?)
    }
}

impl TemplateAble for Template {
    fn get_content(&self) -> &str {
        &self.content
    }
}

impl TemplateAble for &str {
    fn get_content(&self) -> &str {
        self
    }
}

impl Device {
    pub fn get_render_obj(&self) -> Object {
        liquid::object!({
            "width": self.width,
            "height": self.height,
            "fw_version": self.fw_version,
        })
    }
}
