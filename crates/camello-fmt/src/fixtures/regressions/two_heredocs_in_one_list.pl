# Two indented heredocs among the values of one hash, on separate lines of a
# list that breaks. Each body belongs to the line its marker sits on, and `<<~`
# strips the terminator's own indentation from every body line — so moving the
# terminator changes the string's value, not just its layout.
my $rows = [
    {
        foo => 'a',
        bar => <<~'A',
            alpha
        A
        baz => <<~'A',
            beta
        A
    },
];
