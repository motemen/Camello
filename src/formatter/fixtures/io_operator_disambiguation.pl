# IO operator vs comparison disambiguation tests

# Case 1: IO operator as function argument (issue #203)
decode <$fh>;

# Case 2: Valid chained comparison
$result = f < $x > 1;

# Case 3: Simple IO operator
$line = <STDIN>;

# Case 4: Empty IO operator (null filehandle)
$line = <>;

# Case 5: IO operator with variable filehandle
$data = <$handle>;

# Case 6: Multiple IO operators in sequence
while (<$fh>) {
    process <STDERR>;
}

# Case 7: IO operator in function call with multiple arguments
process <$in>, $output;

# Case 8: Comparison that should NOT be treated as IO
if ($a < $b && $c > $d) {
    print "ok\n";
}

# Case 9: Chained comparison with multiple operators
$test = $a < $b > 0;

# Case 10: IO operator in print-like statement
print <DATA>;
