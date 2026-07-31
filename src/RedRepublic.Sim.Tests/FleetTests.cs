namespace RedRepublic.Sim.Tests;

/// <summary>Journeys, and the vehicles that make them.</summary>
public sealed class FleetTests
{
    private static Tables T => Fixtures.Tables;

    private static Journey Straight(double now = 0.0) =>
        Journey.Begin(
            [0.0, 1000.0, 3000.0],
            [0.0, 0.0, 0.0],
            [Units.KphToMps(50.0), -1.0],
            now,
            Journey.LegTicks(1000.0, Units.KphToMps(50.0), T));

    /// <summary>
    /// <b>Leg ends are absolute, so nothing drifts.</b> A remaining duration that
    /// is decremented accumulates a rounding error over a long haul, and a lorry
    /// that arrives a few ticks late every trip is a republic quietly running
    /// slower than its own timetable.
    /// </summary>
    [Fact]
    public void Leg_ends_are_absolute_and_do_not_drift()
    {
        var j = Straight(now: 100.0);
        var firstEnd = j.LegEnd;

        Assert.Equal(2, j.Legs);
        Assert.Equal(0, j.Leg);
        Assert.Equal(100.0, j.LegStart);
        Assert.False(j.OnLastLeg);
        Assert.False(j.LegDoneBy(firstEnd - 0.001));
        Assert.True(j.LegDoneBy(firstEnd));

        j.Advance(Units.KphToMps(30.0), T);

        Assert.Equal(1, j.Leg);
        Assert.True(j.OnLastLeg);

        // The next leg starts exactly where the last ended — no gap, no overlap.
        Assert.Equal(firstEnd, j.LegStart);
        Assert.Equal(2000.0, j.LegDistance);
        Assert.Throws<InvalidOperationException>(() => j.Advance(10.0, T));
    }

    /// <summary>
    /// A journey has somewhere to go, and every leg says what the way allows on
    /// it. Both are refused rather than defaulted, because a silently-empty
    /// limit list would make every leg cross-country.
    /// </summary>
    [Fact]
    public void A_journey_needs_somewhere_to_go()
    {
        Assert.Throws<ArgumentException>(() => Journey.Begin([0.0], [0.0], [], 0.0, 1.0));
        Assert.Throws<ArgumentException>(
            () => Journey.Begin([0.0, 1.0], [0.0, 1.0], [], 0.0, 1.0));
    }

    /// <summary>
    /// A road's whole point is that a better surface carries faster: a lorry on
    /// a dirt track makes the track's speed whatever it is capable of, and the
    /// same lorry on tarmac makes its own.
    /// </summary>
    [Fact]
    public void The_way_holds_a_vehicle_down_and_the_going_holds_it_down_further()
    {
        var j = Straight();
        var fast = Units.KphToMps(80.0);
        var cross = Units.KphToMps(15.0);

        // Leg 0 has a 50 km/h limit: the lorry cannot exceed the road.
        Assert.True(j.LegOnRoad(0));
        Assert.Equal(Units.KphToMps(50.0), j.SpeedOn(0, fast, cross, 1.0));

        // A slower lorry is held down by itself, not by the road.
        Assert.Equal(Units.KphToMps(30.0), j.SpeedOn(0, Units.KphToMps(30.0), cross, 1.0));

        // Leg 1 has no way under it at all: cross-country pace.
        Assert.False(j.LegOnRoad(1));
        Assert.Equal(cross, j.SpeedOn(1, fast, cross, 1.0));

        // Mud slows both, and drag below one never speeds anything up.
        Assert.Equal(Units.KphToMps(25.0), j.SpeedOn(0, fast, cross, 2.0));
        Assert.Equal(Units.KphToMps(50.0), j.SpeedOn(0, fast, cross, 0.5));
    }

    /// <summary>
    /// A leg never takes less than a tick. Without that floor, a vehicle whose
    /// waypoints happen to be close together crosses the whole republic inside
    /// one tick — which is how a fleet ends up teleporting.
    /// </summary>
    [Fact]
    public void A_leg_never_takes_less_than_a_tick()
    {
        Assert.Equal(T.MinLegTicks, Journey.LegTicks(0.1, Units.KphToMps(50.0), T));
        Assert.Equal(T.MinLegTicks, Journey.LegTicks(1000.0, 0.0, T));
        Assert.True(Journey.LegTicks(100_000.0, Units.KphToMps(50.0), T) > T.MinLegTicks);
    }

    /// <summary>
    /// Position is interpolated along the leg, so a vehicle can be drawn between
    /// simulation steps without the simulation running at the frame rate.
    /// </summary>
    [Fact]
    public void A_vehicle_is_somewhere_between_the_ends_of_its_leg()
    {
        var j = Straight();
        var span = j.LegEnd - j.LegStart;

        Assert.Equal((0.0, 0.0), j.PositionAt(j.LegStart));
        Assert.Equal((1000.0, 0.0), j.PositionAt(j.LegEnd));

        var (halfX, _) = j.PositionAt(j.LegStart + (span / 2.0));
        Assert.Equal(500.0, halfX, 9);

        // Before and after the leg it clamps to the ends rather than running on.
        Assert.Equal((0.0, 0.0), j.PositionAt(j.LegStart - 50.0));
        Assert.Equal((1000.0, 0.0), j.PositionAt(j.LegEnd + 50.0));

        Assert.Equal(3000.0, j.Distance);
        Assert.Equal(3000.0, j.DestinationX);
    }

    /// <summary>
    /// A stuck lorry keeps its job and its plan. What it was doing is remembered
    /// so it can carry on doing it, and the day is recorded because how long it
    /// has been there is what decides whether a tow is sent.
    /// </summary>
    [Fact]
    public void A_bogged_vehicle_remembers_what_it_was_doing()
    {
        var f = new Fleet(T);
        var lorry = f.Commission(Array.IndexOf(T.VehicleIds, "Lorry"), 1, 100.0, 100.0);

        f.SetJob(lorry, Job.Haul(1, Destination.Building(2), T.ResourceIndex("Coal"), 8.0));
        f.SetState(lorry, VehicleState.Delivering);
        Assert.Equal(1, f.Running());

        f.Bog(lorry, day: 42);

        Assert.True(f.IsBogged(lorry));
        Assert.Equal(VehicleState.Delivering, f.DoingAt(lorry));
        Assert.Equal(42, f.BoggedSinceAt(lorry));
        Assert.Equal(1, f.Bogged());
        Assert.Equal(0, f.Running());

        // Its job survives being stuck — that is the point of keeping the plan.
        Assert.Equal(JobKind.Haul, f.JobAt(lorry).Kind);

        // Bogging again does not overwrite what it was doing with "bogged".
        f.Bog(lorry, day: 44);
        Assert.Equal(VehicleState.Delivering, f.DoingAt(lorry));
        Assert.Equal(42, f.BoggedSinceAt(lorry));

        f.Unbog(lorry);
        Assert.Equal(VehicleState.Delivering, f.StateAt(lorry));
        Assert.Equal(1, f.Running());

        // Getting stuck is not something a caller can set directly, so the day
        // can never go unrecorded.
        Assert.Throws<ArgumentException>(() => f.SetState(lorry, VehicleState.Bogged));
    }

    /// <summary>
    /// A garage keeps an establishment: as many vehicles as it owns, whatever
    /// its roster. Fuel is capped by the tank rather than by the delivery.
    /// </summary>
    [Fact]
    public void A_garage_keeps_what_it_owns()
    {
        var f = new Fleet(T);
        var lorry = Array.IndexOf(T.VehicleIds, "Lorry");
        var bus = Array.IndexOf(T.VehicleIds, "CrewBus");

        var a = f.Commission(lorry, 10, 0.0, 0.0);
        f.Commission(lorry, 10, 0.0, 0.0);
        f.Commission(bus, 20, 0.0, 0.0);

        Assert.Equal(3, f.Count);
        Assert.Equal(2, f.OfGarage(10).Count);
        Assert.Single(f.OfGarage(20));
        Assert.Equal(2, f.CountOfKind(lorry));

        // It starts with a full tank, and cannot be given more than one.
        Assert.Equal(T.VTank[lorry], f.FuelAt(a));
        f.SetFuel(a, 1000.0);
        Assert.Equal(T.VTank[lorry], f.FuelAt(a));
        f.SetFuel(a, -5.0);
        Assert.Equal(0.0, f.FuelAt(a));
    }

    /// <summary>
    /// Cargo rides with the vehicle and the table stays in step when one is
    /// scrapped — the same alignment rule the stockpiles have.
    /// </summary>
    [Fact]
    public void Cargo_follows_the_vehicle_it_is_on()
    {
        var f = new Fleet(T);
        var lorry = Array.IndexOf(T.VehicleIds, "Lorry");
        var coal = T.ResourceIndex("Coal");

        var first = f.Commission(lorry, 1, 0.0, 0.0);
        var second = f.Commission(lorry, 1, 0.0, 0.0);
        var third = f.Commission(lorry, 1, 0.0, 0.0);
        f.Cargo.Add(first, coal, 3.0);
        f.Cargo.Add(third, coal, 7.0);

        Assert.Equal(10.0, f.CargoAfloat());

        var thirdId = f.IdAt(third);
        f.RemoveAt(second);

        var moved = f.IndexOf(thirdId);
        Assert.Equal(1, moved);
        Assert.Equal(7.0, f.Cargo.Get(moved, coal));
        Assert.Equal(3.0, f.Cargo.Get(0, coal));
        Assert.Equal(10.0, f.CargoAfloat());
    }

    /// <summary>
    /// A job says what is being done, and the destination says what sort of
    /// thing it is addressed to — a building, a site under construction, or for
    /// a plough a bare stretch of ground, which is not a consignee at all.
    /// </summary>
    [Fact]
    public void A_job_names_what_it_is_addressed_to()
    {
        var coal = T.ResourceIndex("Coal");

        var haul = Job.Haul(1, Destination.Building(2), coal, 8.0);
        Assert.Equal(JobKind.Haul, haul.Kind);
        Assert.Equal(DestinationKind.Building, haul.To.Kind);
        Assert.Equal(8.0, haul.Tonnes);

        var gravel = Job.Haul(1, Destination.RoadSite(5), T.ResourceIndex("Gravel"), 20.0);
        Assert.Equal(DestinationKind.RoadSite, gravel.To.Kind);

        var plough = Job.Plough(1500.0, 2500.0);
        Assert.Equal(JobKind.Plough, plough.Kind);
        Assert.Equal(DestinationKind.Place, plough.To.Kind);
        Assert.Equal(1500.0, plough.To.X);
        Assert.Equal(-1, plough.To.Id);

        Assert.Equal(JobKind.None, Job.None.Kind);
    }
}
