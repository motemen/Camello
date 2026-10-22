# A multi-line `qw(...)` (INDENT-2). With no line break after the opening
# delimiter, the first word is tight against it, as the last is against the
# closing one. Blank lines inside stay, as they do inside a bracket, and a run
# of them is one (BLANK_LINE-3).
my @a = qw(FOO BAR
    BAZ);
my @b = qw(FOO BAR
    BAZ);

my @c = qw(

    FOO
    BAR

    BAZ

);
