namespace RedRepublic.Sim;

/// <summary>
/// How well the republic is serving the people in one home, component by
/// component.
/// </summary>
/// <remarks>
/// <para>
/// <b>Stored as the breakdown rather than the score</b>, because "your people
/// are at 61%" is not something a player can act on and "fed, warm, no doctor,
/// no work" is. <see cref="Worst"/> is the figure a panel should lead with.
/// </para>
/// <para>
/// <b>A want is a way to fail; a comfort is a way to do better.</b> Everything in
/// <see cref="Parts"/> is a way for a republic to fail its people: absent, it
/// costs. Comforts are the opposite — worth having, and nobody's life is ruined
/// without them — so they are applied as a lift on top rather than as a ninth
/// thing to be short of. That distinction is what makes them addable at all:
/// modelled as a want they would have dropped the score of every republic
/// already standing, on the day the goods were invented, for a shortfall that
/// did not exist the day before. A republic must never be re-marked for work it
/// did before the rules changed.
/// </para>
/// </remarks>
public readonly record struct Contentment(
    double Provisions,
    double Warmth,
    double Health,
    double Culture,
    double Schooling,
    double Work,
    double Cleanliness,
    double Safety,
    double Comforts)
{
    /// <summary>
    /// What each want is called. Authored beside the weights because the panel
    /// reads them together, and because a component with no name cannot be the
    /// answer to "what is worst".
    /// </summary>
    public static readonly string[] Names =
    [
        "Provisions",
        "Warmth",
        "Health",
        "Culture",
        "Schooling",
        "Work",
        "Cleanliness",
        "Safety",
    ];

    // Where each want sits in Names and Parts. Named rather than written as
    // numbers at the call site, so reordering the breakdown is one edit here
    // instead of a hunt through the passes that fill it in.
    public const int ProvisionsSlot = 0;
    public const int WarmthSlot = 1;
    public const int HealthSlot = 2;
    public const int CultureSlot = 3;
    public const int SchoolingSlot = 4;
    public const int WorkSlot = 5;
    public const int CleanlinessSlot = 6;
    public const int SafetySlot = 7;

    /// <summary>A home nobody has done anything for.</summary>
    public static Contentment Nothing { get; }

    /// <summary>The wants, in the order the weights and names are in.</summary>
    public double[] Parts =>
        [Provisions, Warmth, Health, Culture, Schooling, Work, Cleanliness, Safety];

    /// <summary>
    /// The weighted share of the wants that are met.
    /// </summary>
    /// <remarks>
    /// The weights come from the balance table rather than from a constant here.
    /// How much food matters against how much a cinema matters is a decision
    /// somebody made, and a list of numbers inside logic is the thing you forget
    /// to edit — food and warmth dominate because they are the two a person
    /// cannot do without, and that is balance rather than arithmetic.
    /// </remarks>
    public double NeedsMet(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var weights = t.ContentmentWeights;
        var total = 0.0;
        var scored = 0.0;
        var parts = Parts;
        for (var i = 0; i < parts.Length; i++)
        {
            total += weights[i];
            scored += Math.Clamp(parts[i], 0.0, 1.0) * weights[i];
        }

        return total <= 0.0 ? 0.0 : Math.Clamp(scored / total, 0.0, 1.0);
    }

    /// <summary>
    /// The most that fully-stocked comforts add. It certainly helps and it is
    /// not a dealbreaker, which is the whole brief.
    /// </summary>
    public double Lift(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return t.ComfortLift * Math.Clamp(Comforts, 0.0, 1.0);
    }

    public double Overall(Tables t) => Math.Clamp(NeedsMet(t) + Lift(t), 0.0, 1.0);

    /// <summary>
    /// The want costing this home the most, or null if it wants for nothing.
    /// </summary>
    /// <remarks>
    /// <b>Tell the player which thing is worst, not what their score is.</b>
    /// Ranked by weighted loss, so a small shortfall in food outranks a total
    /// absence of cinemas. Ties break by order so the answer is stable rather
    /// than flickering between two equally bad things.
    /// <para>
    /// Comforts can never be the answer: "your people's biggest problem is no
    /// television" is not something a panel should say to somebody whose estate
    /// is cold.
    /// </para>
    /// </remarks>
    public string? Worst(Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        var weights = t.ContentmentWeights;
        var parts = Parts;
        var worst = -1;
        var worstLoss = 1e-9;
        for (var i = 0; i < parts.Length; i++)
        {
            var loss = (1.0 - Math.Clamp(parts[i], 0.0, 1.0)) * weights[i];
            if (loss > worstLoss)
            {
                worstLoss = loss;
                worst = i;
            }
        }

        return worst < 0 ? null : Names[worst];
    }
}
