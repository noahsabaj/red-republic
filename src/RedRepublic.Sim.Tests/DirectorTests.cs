namespace RedRepublic.Sim.Tests;

/// <summary>
/// The opening, played.
/// </summary>
/// <remarks>
/// <para>
/// <b>The gate's one claim about whether the game works at all.</b> Everything
/// else in this suite is about a rule; this founds a republic on real generated
/// ground, hands it to the reference player, and asks whether two years later
/// anything is happening.
/// </para>
/// <para>
/// <see cref="Director"/> lives in the simulation rather than in the trajectory
/// runner precisely so this can exist, and for the life of the port nothing
/// constructed one — the gate stayed green through a decade in which no coal was
/// cut, no rouble was earned and eleven buildings sat dark, because nothing
/// asked.
/// </para>
/// <para>
/// The thresholds are deliberately low. The director is a bad player and a
/// failure under it does not prove the game unwinnable, so this asserts only
/// what a <i>broken</i> republic could not manage: that the plan gets built,
/// that somebody lives there, that the lights come on, and that the ground gets
/// dug. Reading a trajectory is still how balance is judged; this is only here
/// to say when there is nothing left to read.
/// </para>
/// </remarks>
public sealed class DirectorTests
{
    private const int Years = 2;

    [Fact]
    public void The_reference_opening_builds_a_republic_that_does_something()
    {
        var tables = Fixtures.Tables;
        var world = World.Found(new WorldSpec(1961, 4_000.0, 0), tables);
        var (cx, cy) = Scenario.Found(world);
        var director = new Director(cx, cy);

        var coalUnderfoot = world.Geology.RemainingOf(Mineral.Coal);

        for (var month = 0; month < Years * SimClock.MonthsPerYear; month++)
        {
            director.Month(world);
            for (var tick = 0; tick < SimClock.TicksPerDay * SimClock.DaysPerMonth; tick++)
            {
                world.Tick();
            }
        }

        var standing = 0;
        var lit = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            standing++;
            if (tables.BPowerDraw[world.Buildings.KindAt(b)] > 0.0
                && world.Buildings.PoweredAt(b))
            {
                lit++;
            }
        }

        Assert.True(standing >= 8, Why(world, director, $"only {standing} buildings stand"));
        Assert.True(world.Citizens.Count > 0, Why(world, director, "nobody lives here"));
        Assert.True(lit > 0, Why(world, director, "nothing in the republic has current"));

        // The ground has been dug. A republic that cut no coal in two years with
        // a pit, a plant and a grid is one where something is wired wrong rather
        // than one that was played badly.
        Assert.True(
            world.Geology.RemainingOf(Mineral.Coal) < coalUnderfoot,
            Why(world, director, "not one tonne came out of the ground"));

        // And somebody is being fed, which is the whole point of the food chain
        // and the freight that serves it.
        var provisioned = 0.0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            provisioned = Math.Max(provisioned, world.Buildings.ProvisionedAt(b));
        }

        Assert.True(provisioned > 0.0, Why(world, director, "no estate has been provisioned"));
    }

    /// <summary>
    /// The failure message: what the director said, and what every building is
    /// doing.
    /// </summary>
    /// <remarks>
    /// A bare "expected true" here costs an afternoon. Which building is short
    /// of what is the answer, and the republic is standing right there.
    /// </remarks>
    private static string Why(World world, Director director, string complaint)
    {
        var t = world.Tables;
        var lines = new List<string> { complaint, string.Empty };
        lines.AddRange(director.Said);
        lines.Add(string.Empty);

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            var kind = world.Buildings.KindAt(b);
            lines.Add(
                $"{t.BName[kind],-24} {world.Buildings.StaffAt(b),3}/{world.Buildings.Jobs(b),-3} "
                + $"{(world.Buildings.IsBuilt(b) ? Systems.StallReason(world, b).ToString() : "building")}");
        }

        return string.Join("\n", lines);
    }
}
