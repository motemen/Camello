# What a multi-line `qw(...)` keeps (INDENT-2). Space written after the opening
# delimiter on the first line is one space, as on a `qw` of one line
# (SPACING-7), and none stays none. Blank lines inside stay, as they do inside
# a bracket, and a run of them is one (BLANK_LINE-3).
my @a = qw(  FOO BAR
BAZ);
my @b = qw(FOO BAR
BAZ);

my @c = qw(


FOO
BAR

BAZ

);
