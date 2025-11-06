sub logical_alignment {
    # Same operator && should align
    my $result = $foo && $bar;
    my $fallback_longer = $baz && $default_value;
    my $x = $a && $b;

    # Same operator || should align
    $short = $c || $d;
    $very_long_variable = $check || $fallback;
    $y = $e || $f;

    # Same operator // should align
    my $val = $input // get_default();
    my $longer_name = $value // $alternative;

    # Statement modifier with &&
    do_something() && say "ok";
    other_action() && warn "done";

    # Mixed operators should NOT align
    my $mixed1 = $a && $b;
    my $mixed2 = $c || $d;
    my $mixed3 = $e // $f;

    # Multiple operators on same line should not align
    my $multi = $a && $b && $c;
    my $single = $x || $y;
}
