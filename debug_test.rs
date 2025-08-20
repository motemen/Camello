use crate::parser::parse;

fn main() {
    let input = "{ foo(); bar() }";
    let (green, errors) = parse(input);
    
    println!("Input: {}", input);
    println!("Errors: {:?}", errors);
    println!("CST: {:?}", green);
}