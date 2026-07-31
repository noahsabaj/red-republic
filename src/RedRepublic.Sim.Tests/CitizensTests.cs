namespace RedRepublic.Sim.Tests;

/// <summary>
/// People: what stage of life they are at, what they were taught, and where
/// they live.
/// </summary>
public sealed class CitizensTests
{
    private static Tables T => Fixtures.Tables;

    /// <summary>
    /// A student is a working-age adult who is <i>not</i> working, and that is
    /// what makes a university a cost as well as an investment.
    /// </summary>
    [Fact]
    public void A_student_is_a_worker_the_republic_does_not_have()
    {
        var p = new Citizens(T);
        var i = p.AddArrival(1, 18);

        Assert.True(p.CanWork(i));
        Assert.Equal(LifeStage.Worker, p.StageAt(i));

        p.SetStudying(i, true);
        Assert.False(p.CanWork(i));
        Assert.Equal(LifeStage.Student, p.StageAt(i));
        Assert.Equal(1, p.ByStage(LifeStage.Student));
        Assert.Equal(0, p.ByStage(LifeStage.Worker));
    }

    [Theory]
    [InlineData(3, LifeStage.Infant)]
    [InlineData(5, LifeStage.Infant)]
    [InlineData(6, LifeStage.Pupil)]
    [InlineData(15, LifeStage.Pupil)]
    [InlineData(16, LifeStage.Worker)]
    [InlineData(59, LifeStage.Worker)]
    [InlineData(60, LifeStage.Retired)]
    [InlineData(80, LifeStage.Retired)]
    public void Age_decides_the_stage_of_life(int age, LifeStage stage)
    {
        var p = new Citizens(T);
        var i = p.AddArrival(1, age);
        Assert.Equal(stage, p.StageAt(i));
        Assert.Equal(stage == LifeStage.Worker, p.CanWork(i));
    }

    /// <summary>
    /// Attainment is a function of days attended. A republic that never builds a
    /// school raises a generation that cannot run its own mines.
    /// </summary>
    [Fact]
    public void Schooling_is_counted_in_days_attended()
    {
        var p = new Citizens(T);
        var child = p.Add(1, 7, 0, 1.0, 0.6);

        Assert.Equal(Education.Unschooled, p.EducationAt(child));

        for (var d = 0; d < T.SchoolDays; d++)
        {
            p.AddSchoolDay(child);
        }

        Assert.Equal(Education.Schooled, p.EducationAt(child));

        for (var d = 0; d < T.UniversityDays; d++)
        {
            p.AddSchoolDay(child);
        }

        Assert.Equal(Education.Graduate, p.EducationAt(child));

        // Somebody Moscow sends with a posting arrives schooled — the bar the
        // next generation has to be given a school to clear.
        var arrival = p.AddArrival(1, 30);
        Assert.Equal(Education.Schooled, p.EducationAt(arrival));
    }

    /// <summary>
    /// <b>Ageing is spread over the year.</b> A cohort that ages on one day is a
    /// cohort that dies on one day, and a population graph with that sawtooth in
    /// it is a modelling artefact a player would rightly read as a bug.
    /// </summary>
    [Fact]
    public void Birthdays_are_spread_across_the_year()
    {
        var p = new Citizens(T);
        var seen = new HashSet<int>();
        for (var i = 0; i < SimClock.DaysPerYear * 2; i++)
        {
            seen.Add(p.BirthdayAt(p.AddArrival(1, 30)));
        }

        // Every day of the year is somebody's birthday.
        Assert.Equal(SimClock.DaysPerYear, seen.Count);
        foreach (var day in seen)
        {
            Assert.InRange(day, 0, SimClock.DaysPerYear - 1);
        }
    }

    /// <summary>
    /// The census is counted in a single walk of the population. Asking each
    /// home in turn would be a walk per home, which is a republic squared — and
    /// the contentment pass wants this for every home every day.
    /// </summary>
    [Fact]
    public void The_census_counts_every_home_in_one_walk()
    {
        var p = new Citizens(T);

        // Two homes: one with a working couple and a child, one with a pensioner.
        var a = p.AddArrival(10, 30);
        var b = p.AddArrival(10, 28);
        p.Add(10, 8, 0, 1.0, 0.5);
        p.AddArrival(20, 70);

        p.SetWorkplace(a, 99, Commute.OnFoot(500.0, T));

        var census = p.CensusByHome();
        Assert.Equal(2, census.Count);

        var home = census[10];
        Assert.Equal(3, home.Residents);
        Assert.Equal(2, home.WorkingAge);
        Assert.Equal(1, home.Employed);
        Assert.Equal(1, home.Children);
        Assert.InRange(home.Health, 0.0, 1.0);

        var other = census[20];
        Assert.Equal(1, other.Residents);
        Assert.Equal(0, other.WorkingAge);

        Assert.Equal(1, p.Employed());
        Assert.Equal(0, p.Riders());
        Assert.Equal(b, p.IndexOfId(p.IdAt(b)));
    }

    /// <summary>
    /// A commute on foot is distance over an unhurried pace; a ride needs a
    /// seat. The distinction is what makes a bus depot worth building.
    /// </summary>
    [Fact]
    public void A_walk_and_a_ride_are_different_journeys()
    {
        var p = new Citizens(T);
        var walker = p.AddArrival(1, 30);
        var rider = p.AddArrival(1, 30);

        var onFoot = Commute.OnFoot(1000.0, T);
        Assert.Equal(CommuteMode.Foot, onFoot.Mode);
        Assert.False(onFoot.IsCarried);
        Assert.Equal(Units.TimeToCover(Units.KphToMps(T.WalkKph), 1000.0), onFoot.Time);

        var carried = Commute.Carried(0, 6000.0, 900.0);
        Assert.True(carried.IsCarried);

        p.SetWorkplace(walker, 5, onFoot);
        p.SetWorkplace(rider, 5, carried);

        Assert.Equal(1, p.Riders());
        Assert.Equal(2, p.Employed());

        Assert.Equal(CommuteMode.None, Commute.None.Mode);
        Assert.False(Commute.None.IsCarried);
    }

    [Fact]
    public void Removing_somebody_leaves_everybody_else_intact()
    {
        var p = new Citizens(T);
        var first = p.AddArrival(1, 30);
        var second = p.AddArrival(2, 40);
        var third = p.AddArrival(3, 50);
        p.SetHealth(third, 0.25);

        var thirdId = p.IdAt(third);
        p.RemoveAt(second);

        Assert.Equal(2, p.Count);
        Assert.Equal(1, p.HomeAt(first));
        var moved = p.IndexOfId(thirdId);
        Assert.Equal(1, moved);
        Assert.Equal(3, p.HomeAt(moved));
        Assert.Equal(0.25, p.HealthAt(moved));
    }
}
