using RedRepublic.Sim;

namespace RedRepublic.Trajectory;

/// <summary>
/// The headless trajectory runner: the only thing that exercises the opening.
/// </summary>
/// <remarks>
/// <para>
/// It founds a republic on a real generated map, hands it to
/// <see cref="Director"/> — a deliberately plain player — and prints what
/// happened month by month.
/// </para>
/// <para>
/// <b>Nothing here asserts.</b> A trajectory is evidence, and reading it is the
/// point: if the coal column goes flat in year two, that is the balance telling
/// you something, and no threshold in a test would have said it as clearly. Its
/// director is deliberately a bad player, so failure under it does not prove the
/// game unwinnable.
/// </para>
/// <para>
/// <b>A posting, not a town.</b> It plays the opening from an empty map and a
/// rouble grant. Standing the test fixture up instead would make every figure it
/// prints a measurement of a republic nobody is given.
/// </para>
/// </remarks>
internal static class Program
{
    private static int Main(string[] args)
    {
        var seed = args.Length > 0 && ulong.TryParse(args[0], out var s) ? s : 1961UL;
        var years = args.Length > 1 && int.TryParse(args[1], out var y) ? y : 3;
        var climateId = args.Length > 2 ? args[2] : "plains";

        var manifest = Path.Combine(RepoRoot(), "game", "data", "manifest.json");
        if (!File.Exists(manifest))
        {
            Console.Error.WriteLine($"no balance table at {manifest}");
            return 1;
        }

        var tables = Tables.Load(File.ReadAllText(manifest));
        var climate = Climate(tables, climateId);

        var world = World.Found(new WorldSpec(seed, 6_000.0, climate), tables);
        var (cx, cy) = Scenario.Found(world);
        var director = new Director(cx, cy);

        Console.WriteLine("Red Republic — trajectory");
        Console.WriteLine(
            $"seed {seed} · {years} years · 6 km republic · {tables.Climates[climate].Name}");
        Console.WriteLine(
            $"posted at ({cx:F0} m, {cy:F0} m) · nothing built · "
            + $"{Scenario.GrantRoubles:F0} roubles");
        Console.WriteLine(
            $"coal in the ground at founding: {world.Geology.RemainingOf(Mineral.Coal):F0} t");
        Console.WriteLine();

        Console.WriteLine(
            $"{"date",10} {"pop",4} {"empl",5} {"fed%",5} {"degC",6} {"warm%",6} "
            + $"{"coal",8} {"food",8} {"fuel",7} {"lorries",7} {"road km",8} "
            + $"{"grid km",8} {"money",10} {"dark",5} {"crew/wait",10} {"cont%",6} "
            + $"{"waiting",8}");

        var fuel = tables.ResourceIndex("Fuel");
        var coal = tables.ResourceIndex("Coal");
        var food = tables.ResourceIndex("Food");
        var power = tables.UtilityIndex("Power");

        for (var month = 0; month < years * SimClock.MonthsPerYear; month++)
        {
            director.Month(world);

            for (var day = 0; day < SimClock.DaysPerMonth; day++)
            {
                for (var tick = 0; tick < SimClock.TicksPerDay; tick++)
                {
                    world.Tick();
                }
            }

            var date = world.Clock.Date;
            var (employed, content) = People(world);
            var (dark, warm, fed) = Buildings(world, power);
            var (crew, waiting) = Crews(world);

            Console.WriteLine(
                $"{date.Year,4}-{date.Month:00}-{date.Day:00} "
                + $"{world.Citizens.Count,4} {employed,5} {fed * 100.0,4:F0}% "
                + $"{world.Climate.MeanOn(world.Clock.DayOfYear),6:F1} "
                + $"{warm * 100.0,5:F0}% "
                + $"{Held(world, coal),8:F0} {Held(world, food),8:F0} {Held(world, fuel),7:F1} "
                + $"{world.Fleet.Count,7} {world.Roads.TotalLength() / 1000.0,8:F1} "
                + $"{world.Grid.LengthOf(power) / 1000.0,8:F1} "
                + $"{world.Treasury.Of(Market.East),10:F0} {dark,5} "
                + $"{crew,4}/{waiting,-5} {content * 100.0,5:F0}% "
                + $"{world.Migration.HeadsWaiting(),8}");

            foreach (var line in director.Drain())
            {
                Console.WriteLine($"  {line}");
            }

        }

        Console.WriteLine();
        Console.WriteLine(
            $"{world.Buildings.Count} buildings · {world.Citizens.Count} people · "
            + $"{world.Roads.SegmentCount} road segments · {world.Grid.Count} spans");

        return 0;
    }

    private static double Held(World world, int resource)
    {
        var total = 0.0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            total += world.Buildings.Stock.Get(b, resource);
        }

        return total;
    }

    /// <summary>
    /// How many hold a job, and how content the republic is.
    /// </summary>
    /// <remarks>
    /// Contentment is weighted by how many live in each estate: one wretched
    /// outpost does not cancel a working city, and an unweighted mean would say
    /// it did.
    /// </remarks>
    private static (int Employed, double Content) People(World world)
    {
        var employed = 0;
        for (var c = 0; c < world.Citizens.Count; c++)
        {
            if (world.Citizens.WorkplaceAt(c) >= 0)
            {
                employed++;
            }
        }

        var scored = 0.0;
        var heads = 0;
        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b)
                || world.Tables.BResidents[world.Buildings.KindAt(b)] == 0)
            {
                continue;
            }

            var here = world.Citizens.ResidentsOf(world.Buildings.IdAt(b));
            scored += world.Buildings.ContentmentAt(b).Overall(world.Tables) * here;
            heads += here;
        }

        return (employed, heads == 0 ? 0.0 : scored / heads);
    }

    /// <summary>How many are unlit, what share is warm, and what share is fed.</summary>
    private static (int Dark, double Warm, double Fed) Buildings(World world, int power)
    {
        var t = world.Tables;
        var dark = 0;
        var wantHeat = 0;
        var warm = 0;
        var homes = 0;
        var fed = 0.0;

        for (var b = 0; b < world.Buildings.Count; b++)
        {
            if (!world.Buildings.IsBuilt(b))
            {
                continue;
            }

            var kind = world.Buildings.KindAt(b);
            if (t.BPowerDraw[kind] > 0.0 && !world.Buildings.PoweredAt(b))
            {
                dark++;
            }

            if (t.BHeat[kind] > 0.0)
            {
                wantHeat++;
                if (world.Buildings.HeatedAt(b))
                {
                    warm++;
                }
            }

            if (t.BResidents[kind] > 0)
            {
                homes++;
                fed += world.Buildings.ProvisionedAt(b);
            }
        }

        _ = power;
        return (dark, wantHeat == 0 ? 1.0 : (double)warm / wantHeat, homes == 0 ? 0.0 : fed / homes);
    }

    /// <summary>
    /// Builders out from their offices, and how many are standing about waiting.
    /// </summary>
    /// <remarks>
    /// The second number is the friction one: a gang with nowhere to be is people
    /// the republic is paying to stand in a field, and it is invisible in every
    /// other column here.
    /// </remarks>
    private static (int Out, int Waiting) Crews(World world)
    {
        var heads = 0;
        var stranded = 0;
        foreach (var party in world.Crews.All)
        {
            heads += party.Heads;
            if (party.IsStranded)
            {
                stranded += party.Heads;
            }
        }

        return (heads, stranded);
    }

    /// <summary>
    /// Which climate to run, by name — the fastest way to see what a winter
    /// costs, because the same republic on the taiga burns a different amount of
    /// coal for the same output.
    /// </summary>
    private static int Climate(Tables tables, string name)
    {
        for (var i = 0; i < tables.Climates.Length; i++)
        {
            if (string.Equals(tables.Climates[i].Id, name, StringComparison.OrdinalIgnoreCase))
            {
                return i;
            }
        }

        return 0;
    }

    /// <summary>
    /// Walks up from the binary until it finds the repository, so the runner
    /// reads the same table the game ships rather than a stale copy beside it.
    /// </summary>
    private static string RepoRoot()
    {
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        while (dir is not null)
        {
            if (Directory.Exists(Path.Combine(dir.FullName, ".git")))
            {
                return dir.FullName;
            }

            dir = dir.Parent;
        }

        throw new DirectoryNotFoundException($"no repository above {AppContext.BaseDirectory}");
    }
}
