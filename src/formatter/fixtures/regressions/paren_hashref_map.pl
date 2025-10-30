# Regression: multiline parentheses inside hash references should indent like other delimiters
my $hash = +{
    (
        map { $_ }
        @list
    )
};
