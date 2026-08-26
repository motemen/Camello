map {
    $_ + 1;
}
sort {
    $a <=> $b;
} @array;

my @names = map {
    $_->{name};
}
grep {
    $_->{ok};
}
@records;

push @signals, map {
    uc $_;
}
     @names;
