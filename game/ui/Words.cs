using RedRepublic.Sim;

namespace RedRepublic.Ui;

/// <summary>
/// What the simulation's vocabulary is called on a screen.
/// </summary>
/// <remarks>
/// <para>
/// <b>An enum member is an identifier, not a sentence.</b> Printing one straight
/// at the player hands them the simulation's own vocabulary — <c>Last</c>,
/// <c>Offered</c>, <c>East</c>, <c>Pupil</c> — and the interface is the game
/// project's entirely, so every word a player reads is chosen here.
/// </para>
/// <para>
/// It is also the only way the words stay in step. The instrument panel already
/// kept a hand-written roster of the life stages and the screens printed the
/// identifiers, so the population read "Pupils" in one place and "Pupil" in the
/// other — two answers to one question, drifting exactly as a second copy always
/// does.
/// </para>
/// <para>
/// This is not the same as a <i>name</i>. A building's name is authored beside
/// the thing it names and crosses the boundary as a string; these have no row in
/// the table to be authored in, because they are the simulation's own structure.
/// </para>
/// </remarks>
public static class Words
{
    /// <summary>Where a workplace stands when the republic is short of hands.</summary>
    public static string Of(Priority standing) => standing switch
    {
        Priority.First => "first call",
        Priority.Ordinary => "ordinary",
        _ => "last call",
    };

    /// <summary>Who is on the other side of the frontier, and whose money it is.</summary>
    public static string Of(Market bloc) => bloc == Market.East ? "Eastern Bloc" : "Western Bloc";

    /// <summary>The short form, for a column too narrow for the long one.</summary>
    public static string Brief(Market bloc) => bloc == Market.East ? "East" : "West";

    /// <summary>What the republic counts in, per bloc.</summary>
    public static string Money(Market bloc) => bloc == Market.East ? "roubles" : "dollars";

    /// <summary>Where somebody is in a life.</summary>
    /// <remarks>Plural, because every place these appear is a count of people.</remarks>
    public static string Of(LifeStage stage) => stage switch
    {
        LifeStage.Infant => "Infants",
        LifeStage.Pupil => "Pupils",
        LifeStage.Student => "Students",
        LifeStage.Worker => "Workers",
        _ => "Retired",
    };

    /// <summary>What their schooling adds up to.</summary>
    public static string Of(Education taught) => taught switch
    {
        Education.Unschooled => "Unschooled",
        Education.Schooled => "Schooled",
        _ => "Graduates",
    };

    /// <summary>Where a tender has got to.</summary>
    public static string Of(ContractState state) => state switch
    {
        ContractState.Offered => "on the table",
        ContractState.Accepted => "being delivered",
        ContractState.Delivered => "delivered",
        ContractState.Failed => "failed",
        _ => "withdrawn",
    };

    /// <summary>Whether goods are going out or coming in.</summary>
    public static string Of(TradeAction action) =>
        action == TradeAction.Sell ? "sell" : "buy";
}
