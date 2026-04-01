use manifest::Viewport;
use serde::Deserialize;

use crate::browser::Browser;
use crate::error::CliError;

#[derive(Debug, Deserialize)]
struct InteractiveElement {
    index: usize,
    tag: String,
    #[serde(default)]
    element_type: Option<String>,
    selector: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    placeholder: Option<String>,
    #[serde(default)]
    aria_label: Option<String>,
    #[serde(default)]
    href: Option<String>,
    #[serde(default)]
    bounds: Option<Bounds>,
}

#[derive(Debug, Deserialize)]
struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

const SCAN_JS: &str = r#"
(() => {
    const INTERACTIVE_QUERY = [
        'a[href]', 'button', 'input', 'textarea', 'select',
        '[role="button"]', '[role="link"]', '[role="checkbox"]',
        '[role="tab"]', '[onclick]', '[tabindex]:not([tabindex="-1"])'
    ].join(', ');

    const ALL_VISIBLE_QUERY = '*';

    function isVisible(el) {
        const style = getComputedStyle(el);
        if (style.display === 'none' || style.visibility === 'hidden') return false;
        const rect = el.getBoundingClientRect();
        return rect.width > 0 && rect.height > 0;
    }

    function uniqueSelector(el) {
        // Try id
        if (el.id) {
            const sel = '#' + CSS.escape(el.id);
            if (document.querySelectorAll(sel).length === 1) return sel;
        }

        const tag = el.tagName.toLowerCase();

        // Try tag + meaningful attributes
        for (const attr of ['name', 'data-testid', 'aria-label', 'href', 'type', 'placeholder']) {
            const val = el.getAttribute(attr);
            if (val) {
                const sel = `${tag}[${attr}="${CSS.escape(val)}"]`;
                if (document.querySelectorAll(sel).length === 1) return sel;
            }
        }

        // Try tag + classes
        if (el.classList.length > 0) {
            const sel = tag + '.' + Array.from(el.classList).map(c => CSS.escape(c)).join('.');
            if (document.querySelectorAll(sel).length === 1) return sel;
        }

        // Path-based: walk up to find a unique ancestor
        let current = el;
        const parts = [];
        while (current && current !== document.body) {
            const ctag = current.tagName.toLowerCase();
            const parent = current.parentElement;
            if (parent) {
                const siblings = Array.from(parent.children).filter(c => c.tagName === current.tagName);
                if (siblings.length > 1) {
                    const idx = siblings.indexOf(current) + 1;
                    parts.unshift(`${ctag}:nth-of-type(${idx})`);
                } else {
                    parts.unshift(ctag);
                }
            } else {
                parts.unshift(ctag);
            }
            // Check if current path is unique
            const candidate = parts.join(' > ');
            if (document.querySelectorAll(candidate).length === 1) return candidate;
            current = parent;
        }

        return parts.join(' > ');
    }

    function scan(allElements) {
        const query = allElements ? ALL_VISIBLE_QUERY : INTERACTIVE_QUERY;
        const elements = Array.from(document.querySelectorAll(query));
        const results = [];

        for (const el of elements) {
            if (!isVisible(el)) continue;
            // Skip html/head/body/script/style for 'all' mode
            const tag = el.tagName.toLowerCase();
            if (['html', 'head', 'body', 'script', 'style', 'meta', 'link', 'noscript'].includes(tag)) continue;

            const rect = el.getBoundingClientRect();
            const text = el.innerText?.trim().substring(0, 80) || null;
            const placeholder = el.getAttribute('placeholder') || null;
            const ariaLabel = el.getAttribute('aria-label') || null;
            const href = el.getAttribute('href') || null;
            const elType = el.getAttribute('type') || null;

            results.push({
                tag,
                element_type: elType,
                selector: uniqueSelector(el),
                text: text || null,
                placeholder,
                aria_label: ariaLabel,
                href,
                bounds: { x: rect.x, y: rect.y, width: rect.width, height: rect.height }
            });
        }

        // Sort by position: top-to-bottom, left-to-right
        results.sort((a, b) => {
            const dy = a.bounds.y - b.bounds.y;
            if (Math.abs(dy) > 5) return dy;
            return a.bounds.x - b.bounds.x;
        });

        // Add index
        results.forEach((r, i) => { r.index = i + 1; });

        return results;
    }

    return JSON.stringify(scan(!!window.__STEPSHOTS_SCAN_ALL));
})()
"#;

pub async fn run(url: &str, width: u32, height: u32) -> Result<(), CliError> {
    let viewport = Viewport { width, height };

    println!("Launching browser...");
    let browser = Browser::launch(&viewport, false).await?;

    println!("Navigating to {url}");
    browser.navigate(url).await?;
    browser.wait_idle(1500).await;

    let mut elements = scan_elements(&browser, false).await?;
    print_table(url, &elements);

    // Interactive REPL
    println!(
        "\nCommands: <number> detail | r refresh | all scan all | nav <url> navigate | q quit"
    );

    loop {
        let line = match read_line_or_ctrl_c().await {
            Some(l) => l,
            None => break, // Ctrl+C
        };

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        match line {
            "q" | "quit" => break,
            "r" | "refresh" => {
                elements = scan_elements(&browser, false).await?;
                print_table(url, &elements);
            }
            "all" => {
                elements = scan_elements(&browser, true).await?;
                print_table(url, &elements);
            }
            cmd if cmd.starts_with("nav ") => {
                let target = cmd.strip_prefix("nav ").unwrap().trim();
                let resolved = resolve_url(url, target);
                println!("Navigating to {resolved}...");
                browser.navigate(&resolved).await?;
                browser.wait_idle(1500).await;
                elements = scan_elements(&browser, false).await?;
                print_table(&resolved, &elements);
            }
            _ => {
                if let Ok(num) = line.parse::<usize>() {
                    if let Some(el) = elements.iter().find(|e| e.index == num) {
                        print_detail(el);
                    } else {
                        println!("No element #{num}. Range: 1-{}", elements.len());
                    }
                } else {
                    println!("Unknown command: {line}");
                    println!(
                        "Commands: <number> detail | r refresh | all scan all | nav <url> navigate | q quit"
                    );
                }
            }
        }
    }

    println!("Bye!");
    Ok(())
}

async fn scan_elements(browser: &Browser, all: bool) -> Result<Vec<InteractiveElement>, CliError> {
    // Set the scan mode flag
    let flag_js = format!(
        "window.__STEPSHOTS_SCAN_ALL = {}",
        if all { "true" } else { "false" }
    );
    browser
        .page()
        .evaluate(flag_js)
        .await
        .map_err(|e| CliError::Browser(format!("Failed to set scan flag: {e}")))?;

    let result = browser
        .page()
        .evaluate(SCAN_JS)
        .await
        .map_err(|e| CliError::Browser(format!("Failed to scan elements: {e}")))?;

    let json_str = result
        .into_value::<String>()
        .map_err(|_| CliError::Browser("Failed to parse scan result as string".into()))?;

    let elements: Vec<InteractiveElement> = serde_json::from_str(&json_str)
        .map_err(|e| CliError::Browser(format!("Failed to deserialize elements: {e}")))?;

    Ok(elements)
}

fn print_table(url: &str, elements: &[InteractiveElement]) {
    let mode = if elements.iter().any(|e| {
        !matches!(
            e.tag.as_str(),
            "a" | "button" | "input" | "textarea" | "select"
        )
    }) {
        " (all visible)"
    } else {
        ""
    };

    println!("\nInteractive elements on {url}{mode}:\n");

    if elements.is_empty() {
        println!("  (no elements found)");
        return;
    }

    // Column widths
    let idx_w = 4;
    let tag_w = 10;
    let sel_w = 50;

    println!(
        "  {:>idx_w$}  | {:<tag_w$} | {:<sel_w$} | Text / Label",
        "#", "Tag", "Selector"
    );
    println!(
        "  {:->idx_w$}--+-{:-<tag_w$}-+-{:-<sel_w$}-+---------------------------",
        "", "", ""
    );

    for el in elements {
        let label = element_label(el);
        let selector = truncate(&el.selector, sel_w);
        println!(
            "  {:>idx_w$}  | {:<tag_w$} | {:<sel_w$} | {}",
            el.index, el.tag, selector, label
        );
    }

    println!("\n  {} elements found.", elements.len());
}

fn print_detail(el: &InteractiveElement) {
    println!("\n  Element #{}", el.index);
    println!("  Tag:         {}", el.tag);
    if let Some(ref t) = el.element_type {
        println!("  Type:        {t}");
    }
    println!("  Selector:    {}", el.selector);
    if let Some(ref t) = el.text {
        println!("  Text:        \"{t}\"");
    }
    if let Some(ref p) = el.placeholder {
        println!("  Placeholder: \"{p}\"");
    }
    if let Some(ref a) = el.aria_label {
        println!("  Aria-label:  \"{a}\"");
    }
    if let Some(ref h) = el.href {
        println!("  Href:        {h}");
    }
    if let Some(ref b) = el.bounds {
        println!(
            "  Bounds:      x={:.0} y={:.0} w={:.0} h={:.0}",
            b.x, b.y, b.width, b.height
        );
    }
    println!();
}

fn element_label(el: &InteractiveElement) -> String {
    if let Some(ref t) = el.text
        && !t.is_empty()
    {
        return format!("\"{}\"", truncate(t, 40));
    }
    if let Some(ref p) = el.placeholder {
        return format!("placeholder: \"{p}\"");
    }
    if let Some(ref a) = el.aria_label {
        return format!("aria: \"{a}\"");
    }
    if let Some(ref h) = el.href {
        return truncate(h, 40).to_string();
    }
    String::new()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn resolve_url(base: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    // Extract origin from base URL
    if let Some(idx) = base.find("://") {
        let after_scheme = &base[idx + 3..];
        let origin_end = after_scheme.find('/').unwrap_or(after_scheme.len());
        let origin = &base[..idx + 3 + origin_end];
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        format!("{origin}{path}")
    } else {
        format!("{base}{path}")
    }
}

async fn read_line_or_ctrl_c() -> Option<String> {
    let line_future = tokio::task::spawn_blocking(|| {
        let mut buf = String::new();
        eprint!("inspect> ");
        match std::io::stdin().read_line(&mut buf) {
            Ok(0) => None,
            Ok(_) => Some(buf),
            Err(_) => None,
        }
    });

    tokio::select! {
        result = line_future => {
            match result {
                Ok(Some(line)) => Some(line),
                _ => None,
            }
        }
        _ = tokio::signal::ctrl_c() => {
            println!();
            None
        }
    }
}
