# A `qw(...)` on one line (SPACING-7): space written after the opening delimiter
# becomes one space at each end, and none written there leaves the words tight
# against both delimiters, whatever was written before the closing one. The words
# are separated by one space. The kind of delimiter makes no difference.
my @a = qw(foo bar);
my @b = qw( foo bar );
my @c = qw( foo bar );
my @d = qw(foo bar);
my @e = qw(foo bar);
my @f = qw( foo bar );
my @g = qw{foo bar};
my @h = qw/ foo bar /;
my @i = qw();
