pub struct Insights {
    contents: Vec<String>,
}

impl Insights {
    pub fn new() -> Self {
        Self {
            contents: Vec::new(),
        }
    }
}

pub trait CollectInsights {
    fn get_insights(&self) -> &[String];
    fn collect_insight(&mut self, content: String);
}

pub trait CollectInsightsDelegator {
    fn target(&self) -> &dyn CollectInsights;
    fn target_mut(&mut self) -> &mut dyn CollectInsights;
}

impl<T> CollectInsights for T
where
    T: CollectInsightsDelegator,
{
    fn get_insights(&self) -> &[String] {
        self.target().get_insights()
    }

    fn collect_insight(&mut self, content: String) {
        self.target_mut().collect_insight(content)
    }
}

impl CollectInsights for Insights {
    fn get_insights(&self) -> &[String] {
        &self.contents
    }

    fn collect_insight(&mut self, content: String) {
        self.contents.push(content);
    }
}
