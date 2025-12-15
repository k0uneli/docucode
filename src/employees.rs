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
            || self.number.to_lowercase().contains(&q)
            || full.contains(&q)
    }
}

pub fn load_employees_from_csv(path: &str) -> csv::Result<Vec<Employee>> {
    let mut rdr = csv::Reader::from_path(path)?;
    let mut out = Vec::new();

    for result in rdr.records() {
        let rec = result?;
        if let Some(emp) = record_to_employee(&rec) {
            out.push(emp);
        }
    }

    Ok(out)
}

fn record_to_employee(rec: &csv::StringRecord) -> Option<Employee> {
    if rec.len() < 3 {
        return None;
    }

    let fields: Vec<String> = rec.iter().map(|s| s.trim().to_string()).collect();
    // Skip header-ish rows that have no digits anywhere
    if !fields.iter().any(|f| f.chars().any(|c| c.is_ascii_digit())) {
        return None;
    }

    let num_idx = fields
        .iter()
        .position(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()));

    let (first, last, number) = match num_idx {
        Some(0) => {
            // [number, last, first]
            let number = fields[0].clone();
            let last = fields.get(1).cloned().unwrap_or_default();
            let first = fields.get(2).cloned().unwrap_or_else(|| last.clone());
            (first, last, number)
        }
        Some(1) => {
            // [first, number, last] or [last, number, first]
            let number = fields[1].clone();
            let first = fields.get(0).cloned().unwrap_or_default();
            let last = fields.get(2).cloned().unwrap_or_else(|| first.clone());
            (first, last, number)
        }
        Some(_) => {
            // assume [first, last, number]
            let number = num_idx.map(|i| fields[i].clone()).unwrap_or_default();
            let first = fields.get(0).cloned().unwrap_or_default();
            let last = fields.get(1).cloned().unwrap_or_default();
            (first, last, number)
        }
        None => {
            // fallback: assume number is last column
            let number = fields.last().cloned().unwrap_or_default();
            let first = fields.get(0).cloned().unwrap_or_default();
            let last = fields.get(1).cloned().unwrap_or_default();
            (first, last, number)
        }
    };

    Some(Employee {
        first,
        last,
        number,
    })
}
