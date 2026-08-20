# A subscript holding one word closes up; one holding an expression or a list
# opens (SPACING-7), the same reading a literal's brackets take.
$h->{key};
$h->{$k};
$a->[0];
$h{a}{b};
$time->[c_sec];
$h->{-key};
$specialsv_name[$$sv];
@Config::Config{@$pair};

$h->{ $o->meth };
$a->[ $i + 1 ];
$h->{ func(1) };
$headers->{ lc $key };
@x{ 'a', 'b' };
@$self{ qw(MAX INDEX) };
$r->@[ 0, 1 ];
$r->%{ a, b };
