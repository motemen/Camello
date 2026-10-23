# A `qw(...)` with no line break after its opening delimiter, written across
# lines (INDENT-2). Its closing delimiter on a line of its own goes back to the
# `qw` line's column, and a bracket closing around it on a line of its own
# stays there — where they go for `(` written the same way.
$EXPORT_TAGS{Parse} = [qw( ParseParameters
Parse_any
)
];

for my $key (qw(LIB LINKTYPE
PREFIX)
) {
    1;
}
