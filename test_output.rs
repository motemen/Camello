use camello::format_code;

fn main() {
    let input = "# This function calculates the answer\nsub answer {\n    return 42;\n}";
    let output = format_code(input);
    println!("{:?}", output);
    println!("---");
    println!("{}", output);
}