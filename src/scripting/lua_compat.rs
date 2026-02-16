pub(crate) fn transpile_lua_compat_to_rhai(source: &str) -> Option<String> {
    let looks_like_lua = source.contains("function update")
        || source.contains("local ")
        || source.contains(" then")
        || source.contains("nil")
        || source.contains("math.sqrt")
        || source.contains("repeat")
        || source.contains(" until ")
        || source.contains("pairs(")
        || source.contains("..");
    if !looks_like_lua {
        return None;
    }

    // Pre-pass: rewrite nested Lua API calls to flat Rhai function names.
    let source = flatten_nested_api_calls(source);

    let mut out = String::new();
    let mut converted_any = false;
    let mut blocks = Vec::<LuaBlock>::new();

    for raw in source.lines() {
        let indent = raw
            .chars()
            .take_while(|c| c.is_ascii_whitespace())
            .collect::<String>();
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if let Some(comment) = trimmed.strip_prefix("--") {
            out.push_str(&indent);
            out.push_str("//");
            out.push_str(comment);
            out.push('\n');
            converted_any = true;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("function ") {
            let rest = rest.trim();
            if let (Some(open), Some(close)) = (rest.find('('), rest.rfind(')')) {
                if close > open {
                    let name = rest[..open].trim();
                    let args = rest[(open + 1)..close].trim();
                    let tail = rest[(close + 1)..].trim();
                    out.push_str(&indent);
                    if tail.is_empty() {
                        out.push_str(&format!("fn {name}({args}) {{\n"));
                        blocks.push(LuaBlock::Standard);
                    } else if let Some(inline) = tail.strip_suffix("end") {
                        out.push_str(&format!("fn {name}({args}) {{\n"));
                        let inline = inline.trim();
                        if !inline.is_empty() {
                            out.push_str(&indent);
                            out.push_str("    ");
                            out.push_str(&ensure_semicolon(&convert_lua_expr(inline)));
                            out.push('\n');
                        }
                        out.push_str(&indent);
                        out.push_str("}\n");
                    } else {
                        out.push_str(&format!("fn {name}({args}) {{\n"));
                    }
                    converted_any = true;
                    continue;
                }
            }
        }

        if let Some(after_if) = trimmed.strip_prefix("if ") {
            if after_if.ends_with(" end") {
                if let Some(then_idx) = after_if.find(" then ") {
                    let cond = convert_lua_condition_expr(after_if[..then_idx].trim());
                    let body =
                        convert_lua_expr(after_if[(then_idx + 6)..(after_if.len() - 4)].trim());
                    out.push_str(&indent);
                    out.push_str(&format!("if {cond} {{\n"));
                    out.push_str(&indent);
                    out.push_str("    ");
                    out.push_str(&ensure_semicolon(&body));
                    out.push('\n');
                    out.push_str(&indent);
                    out.push_str("}\n");
                    converted_any = true;
                    continue;
                }
            }
            if let Some(cond) = after_if.strip_suffix(" then") {
                out.push_str(&indent);
                out.push_str(&format!(
                    "if {} {{\n",
                    convert_lua_condition_expr(cond.trim())
                ));
                blocks.push(LuaBlock::Standard);
                converted_any = true;
                continue;
            }
        }

        if let Some(after_elseif) = trimmed.strip_prefix("elseif ") {
            if let Some(cond) = after_elseif.strip_suffix(" then") {
                out.push_str(&indent);
                out.push_str(&format!(
                    "}} else if {} {{\n",
                    convert_lua_condition_expr(cond.trim())
                ));
                converted_any = true;
                continue;
            }
        }

        if trimmed == "else" {
            out.push_str(&indent);
            out.push_str("} else {\n");
            converted_any = true;
            continue;
        }

        if let Some((name, start, end)) = parse_lua_numeric_for(trimmed) {
            out.push_str(&indent);
            out.push_str(&format!(
                "for {} in {}..={} {{\n",
                name,
                convert_lua_expr(start),
                convert_lua_expr(end)
            ));
            blocks.push(LuaBlock::Standard);
            converted_any = true;
            continue;
        }

        if let Some((k, v, iter)) = parse_lua_pairs_for(trimmed) {
            out.push_str(&indent);
            out.push_str(&format!(
                "for ({}, {}) in {} {{\n",
                k,
                v,
                convert_lua_expr(iter)
            ));
            blocks.push(LuaBlock::Standard);
            converted_any = true;
            continue;
        }

        if let Some(cond) = parse_lua_while(trimmed) {
            out.push_str(&indent);
            out.push_str(&format!("while {} {{\n", convert_lua_condition_expr(cond)));
            blocks.push(LuaBlock::Standard);
            converted_any = true;
            continue;
        }

        if trimmed == "repeat" {
            out.push_str(&indent);
            out.push_str("loop {\n");
            blocks.push(LuaBlock::Repeat);
            converted_any = true;
            continue;
        }

        if let Some(cond) = trimmed.strip_prefix("until ") {
            if matches!(blocks.last(), Some(LuaBlock::Repeat)) {
                let _ = blocks.pop();
                out.push_str(&indent);
                out.push_str("if ");
                out.push_str(&convert_lua_condition_expr(cond.trim()));
                out.push_str(" { break; }\n");
                out.push_str(&indent);
                out.push_str("}\n");
                converted_any = true;
                continue;
            }
        }

        if trimmed == "end" {
            let _ = blocks.pop();
            out.push_str(&indent);
            out.push_str("}\n");
            converted_any = true;
            continue;
        }

        if let Some(local_expr) = trimmed.strip_prefix("local ") {
            let converted = convert_lua_expr(local_expr.trim());
            out.push_str(&indent);
            out.push_str("let ");
            out.push_str(&ensure_semicolon(&converted));
            out.push('\n');
            converted_any = true;
            continue;
        }

        let converted = convert_lua_expr(trimmed);
        out.push_str(&indent);
        out.push_str(&ensure_semicolon(&converted));
        out.push('\n');
    }

    if converted_any {
        Some(out)
    } else {
        None
    }
}

fn convert_lua_expr(expr: &str) -> String {
    convert_lua_expr_with_or(expr, "??")
}

fn convert_lua_condition_expr(expr: &str) -> String {
    convert_lua_expr_with_or(expr, "||")
}

fn convert_lua_expr_with_or(expr: &str, or_replacement: &str) -> String {
    let mut out = expr.replace("~=", "!=").replace("math.sqrt", "sqrt");
    out = replace_concat_operator(&out);
    out = convert_lua_table_literals(&out);
    out = replace_word_token(&out, "nil", "()");
    out = replace_word_token(&out, "and", "&&");
    out = replace_word_token(&out, "or", or_replacement);
    out = replace_word_token(&out, "not", "!");
    out
}

fn ensure_semicolon(line: &str) -> String {
    let line = line.trim_end();
    if line.is_empty() || line.ends_with(';') || line.ends_with('{') || line.ends_with('}') {
        line.to_string()
    } else {
        format!("{line};")
    }
}

fn replace_word_token(input: &str, token: &str, replacement: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let token_chars: Vec<char> = token.chars().collect();
    if token_chars.is_empty() {
        return input.to_string();
    }

    let mut out = String::with_capacity(input.len() + 8);
    let mut i = 0usize;
    while i < chars.len() {
        if i + token_chars.len() <= chars.len()
            && chars[i..(i + token_chars.len())] == token_chars[..]
        {
            let prev_ok = i == 0 || !is_word_char(chars[i - 1]);
            let next_ok =
                i + token_chars.len() == chars.len() || !is_word_char(chars[i + token_chars.len()]);
            if prev_ok && next_ok {
                out.push_str(replacement);
                i += token_chars.len();
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[derive(Clone, Copy)]
enum LuaBlock {
    Standard,
    Repeat,
}

fn parse_lua_numeric_for(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("for ")?.strip_suffix(" do")?.trim();
    if rest.contains(" in ") {
        return None;
    }
    let (name, rhs) = rest.split_once('=')?;
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let mut parts = rhs.split(',').map(str::trim);
    let start = parts.next()?;
    let end = parts.next()?;
    if start.is_empty() || end.is_empty() {
        return None;
    }
    Some((name, start, end))
}

fn parse_lua_pairs_for(line: &str) -> Option<(&str, &str, &str)> {
    let rest = line.strip_prefix("for ")?.strip_suffix(" do")?.trim();
    let (lhs, rhs) = rest.split_once(" in ")?;
    let rhs = rhs.trim();
    let iterable = if let Some(inner) = rhs.strip_prefix("pairs(").and_then(|s| s.strip_suffix(')'))
    {
        inner.trim()
    } else {
        return None;
    };
    let (k, v) = lhs.split_once(',')?;
    let k = k.trim();
    let v = v.trim();
    if k.is_empty() || v.is_empty() || iterable.is_empty() {
        return None;
    }
    Some((k, v, iterable))
}

fn parse_lua_while(line: &str) -> Option<&str> {
    line.strip_prefix("while ")?
        .strip_suffix(" do")
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn replace_concat_operator(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '.' && i + 1 < chars.len() && chars[i + 1] == '.' {
            if i + 2 < chars.len() && chars[i + 2] == '.' {
                out.push('.');
                out.push('.');
                out.push('.');
                i += 3;
            } else {
                out.push('+');
                i += 2;
            }
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn convert_lua_table_literals(input: &str) -> String {
    if !input.contains('{') {
        return input.to_string();
    }
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 4);
    let mut depth = 0usize;
    let mut i = 0usize;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '{' {
            depth += 1;
            out.push('#');
            out.push('{');
            i += 1;
            continue;
        }
        if ch == '}' {
            depth = depth.saturating_sub(1);
            out.push('}');
            i += 1;
            continue;
        }
        if ch == '=' && depth > 0 {
            let prev = chars
                .get(..i)
                .and_then(|slice| slice.iter().rev().find(|c| !c.is_ascii_whitespace()))
                .copied()
                .unwrap_or('\0');
            let next = chars
                .get(i + 1..)
                .and_then(|slice| slice.iter().find(|c| !c.is_ascii_whitespace()))
                .copied()
                .unwrap_or('\0');
            let is_comparison = matches!(prev, '=' | '<' | '>' | '!' | '~') || next == '=';
            if !is_comparison {
                out.push(':');
                i += 1;
                continue;
            }
        }
        out.push(ch);
        i += 1;
    }
    out
}

fn flatten_nested_api_calls(source: &str) -> String {
    const REPLACEMENTS: &[(&str, &str)] = &[
        ("world.camera.shake(", "world.camera_shake("),
        ("world.camera.zoom(", "world.camera_zoom("),
        ("world.camera.look_at(", "world.camera_look_at("),
        ("world.ui.show_screen(", "world.show_screen("),
        ("world.ui.hide_screen(", "world.hide_screen("),
        ("world.ui.set_text(", "world.set_text("),
        ("world.ui.set_progress(", "world.set_progress("),
        ("world.dialogue.start(", "world.start("),
        ("world.dialogue.choose(", "world.choose("),
        ("world.input.pressed(", "world.pressed("),
        ("world.input.just_pressed(", "world.just_pressed("),
        ("world.game.transition(", "world.transition("),
        ("world.game.pause(", "world.pause("),
        ("world.game.resume(", "world.resume("),
        ("world.game.state", "world.game_state"),
    ];
    let mut result = source.to_string();
    for (from, to) in REPLACEMENTS {
        result = result.replace(from, to);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpiles_basic_lua_update_to_rhai() {
        let source = r#"
function update(entity, world, dt)
  local x = entity.x or 0
  if not entity.alive then
    entity.vx = 0
  elseif entity.grounded and x ~= nil then
    entity.vx = math.sqrt(9)
  else
    entity.vx = entity.vx + 1
  end
end
"#;

        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("fn update(entity, world, dt) {"));
        assert!(out.contains("let x = entity.x ?? 0;"));
        assert!(out.contains("if ! entity.alive {") || out.contains("if !entity.alive {"));
        assert!(out.contains("} else if entity.grounded && x != () {"));
        assert!(out.contains("entity.vx = sqrt(9);"));
    }

    #[test]
    fn transpile_skips_non_lua_source() {
        let source = "fn update(entity, world, dt) { entity.vx += 1; }";
        assert!(transpile_lua_compat_to_rhai(source).is_none());
    }

    #[test]
    fn token_replace_is_word_boundary_aware() {
        let source = r#"
function update(entity, world, dt)
  local origin = 1
  local candy = 2
  local orion = origin + candy
  if true or false then
    local ok = true
  end
end
"#;
        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("let origin = 1;"));
        assert!(out.contains("let candy = 2;"));
        assert!(out.contains("let orion = origin + candy;"));
        assert!(out.contains("if true || false {"));
    }

    #[test]
    fn transpiles_single_line_function_end() {
        let source = "function update(entity, world, dt) end";
        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("fn update(entity, world, dt) {"));
        assert!(out.contains("}"));
    }

    #[test]
    fn lua_or_in_assignment_uses_coalesce() {
        let source = r#"
function update(world, dt)
  local score = world.get_var("score") or 0
  world.set_var("score", score + 1)
end
"#;
        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("let score = world.get_var(\"score\") ?? 0;"));
    }

    #[test]
    fn transpiles_lua_loops_and_repeat_until() {
        let source = r#"
function update(world, dt)
  local t = {key = 1}
  for i = 1, 3 do
    world.set_var("i", i)
  end
  for k, v in pairs(t) do
    world.set_var(k, v)
  end
  while world.get_var("ready") ~= nil do
    break
  end
  repeat
    world.set_var("name", "a" .. "b")
  until world.get_var("done")
end
"#;
        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("let t = #{"));
        assert!(out.contains("key"));
        assert!(out.contains(": 1"));
        assert!(out.contains("for i in 1..=3 {"));
        assert!(out.contains("for (k, v) in t {"));
        assert!(out.contains("while world.get_var(\"ready\") != () {"));
        assert!(out.contains("loop {"));
        assert!(out.contains("if world.get_var(\"done\") { break; }"));
        assert!(out.contains("\"a\" + \"b\""));
    }

    #[test]
    fn transpiles_nested_api_calls_to_flat() {
        let source = r#"
function update(entity, world, dt)
  world.camera.shake(5, 0.25)
  world.ui.set_text("score", "100")
  world.dialogue.start("intro")
  if world.input.pressed("left") then
    entity.vx = -100
  end
  world.game.pause()
  local s = world.game.state
end
"#;
        let out = transpile_lua_compat_to_rhai(source).expect("should transpile");
        assert!(out.contains("world.camera_shake(5, 0.25)"));
        assert!(out.contains("world.set_text(\"score\", \"100\")"));
        assert!(out.contains("world.start(\"intro\")"));
        assert!(out.contains("world.pressed(\"left\")"));
        assert!(out.contains("world.pause()"));
        assert!(out.contains("world.game_state"));
    }
}
