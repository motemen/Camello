# A `qw(...)` on one line (SPACING-7): space written inside the delimiters
# becomes one space at each end, on both ends even when it was written on one,
# and none written stays none. The words are separated by one space. The kind
# of delimiter makes no difference.
my @a = qw(foo bar);
my @b = qw( foo bar );
my @c = qw( foo bar );
my @d = qw( foo bar );
my @e = qw(foo bar);
my @f = qw( foo bar );
my @g = qw{foo bar};
my @h = qw/ foo bar /;
my @i = qw();
