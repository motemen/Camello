sub test {
    return $s eq ""
        ? 1
        : 2;
}

sub with_assignment {
    my $value =
        $foo eq "bar"
        ? "foo"
        : "bar";

    return $value;
}
