pub struct Employee {
    pub first: String,
    pub last: String,
    pub number: String,
}

impl Employee {
    pub fn matches_query(&self, q: &str) -> bool {
        let q = q.to_lowercase();
        let full = format!("{} {}", self.first, self.last).to_lowercase();
        self.first.to_lowercase().contains(&q)
            || self.last.to_lowercase().contains(&q)
            || full.contains(&q)
    }
}

pub fn load_employees_from_csv(path: &str) -> csv::Result<Vec<Employee>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut out = Vec::new();

    for result in rdr.records() {
        let rec = result?;
        if rec.len() < 3 {
            continue;
        }
        out.push(Employee {
            first: rec[0].to_string(),
            last: rec[1].to_string(),
            number: rec[2].to_string(),
        });
    }

    Ok(out)
}
