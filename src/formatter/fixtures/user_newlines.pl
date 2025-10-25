# Test user-provided newlines with continuation indent

# if condition with newlines
if (
    $condition1
    && $condition2
) {
    warn "matched";
}

# if with single condition split
if ($very_long_condition
    && $another_part) {
    print "ok";
}

# package with newline
package
    My::Module::Name;

# ternary operator with newlines
my $result = $test
    ? "true_value"
    : "false_value";

# ternary with all parts on separate lines
my $complex =
    $condition
    ? $value1
    : $value2;

# for loop with newlines
for my $item (
    @list
) {
    process($item);
}

# while with condition split
while ($running
    && !$stop_flag) {
    do_work();
}

# unless with newline
unless (
    $skip_condition
) {
    execute();
}

# function call with arguments on multiple lines
my $output = some_function(
    $arg1,
    $arg2
);

# chained method calls with newlines
my $obj = $factory
    ->create()
    ->initialize()
    ->configure();

# hash with fat comma alignment and newlines
my %config = (
    key1 =>
        "long_value_1",
    key2 => "value2",
    key3 =>
        compute_value()
);

# expression continuation with operators
my $sum = $value1
    + $value2
    + $value3
    * $multiplier;

# logical operators
my $allowed = $user->has_permission()
    && $feature->enabled()
    || $is_admin;

# string concatenation
my $message = "Hello, "
    . $name
    . "! Welcome to "
    . $app_name;

# list with newlines in array
my @items = (
    $first,
    $second,
    $third
);

# nested expressions
my $nested = (
    $a + $b
) * (
    $c + $d
);
