namespace RedRepublic.Sim;

/// <summary>What somebody was taught.</summary>
public enum Education
{
    Unschooled,
    Schooled,
    Graduate,
}

/// <summary>
/// What somebody is doing with their life.
/// </summary>
/// <remarks>
/// Derived from age and enrolment rather than stored — a stored copy would be a
/// second source of truth for something age already answers.
/// </remarks>
public enum LifeStage
{
    /// <summary>Too young for school.</summary>
    Infant,

    /// <summary>Of school age.</summary>
    Pupil,

    /// <summary>Of working age and at university instead.</summary>
    Student,

    Worker,
    Retired,
}

/// <summary>How somebody gets to work.</summary>
public enum CommuteMode
{
    None,
    Foot,
    Ride,
}

/// <summary>
/// The journey somebody actually makes to work.
/// </summary>
/// <remarks>
/// Written by the labour pass alongside the workplace, because the two are one
/// decision: <b>a job is only a job if there is a way to get to it.</b>
/// </remarks>
public readonly record struct Commute(CommuteMode Mode, int Medium, double Distance, double Time)
{
    public static Commute None { get; } = new(CommuteMode.None, -1, 0.0, 0.0);

    public static Commute OnFoot(double distance, Tables t)
    {
        ArgumentNullException.ThrowIfNull(t);
        return new Commute(
            CommuteMode.Foot, -1, distance, Units.TimeToCover(Units.KphToMps(t.WalkKph), distance));
    }

    public static Commute Carried(int medium, double distance, double time) =>
        new(CommuteMode.Ride, medium, distance, time);

    /// <summary>Whether this person needs a seat to hold their job.</summary>
    public bool IsCarried => Mode == CommuteMode.Ride;
}

/// <summary>What one home holds, counted in a single walk of the population.</summary>
public record struct HomeCensus(
    int Residents,
    int WorkingAge,
    int Employed,
    int Children,
    double Health,
    double Loyalty);

/// <summary>
/// Everyone in the republic, held as parallel arrays.
/// </summary>
/// <remarks>
/// <para>
/// The daily labour pass walks every citizen; a republic of tens of thousands is
/// the case this has to stay affordable at. Measured on the shape rather than
/// guessed: labour over six thousand citizens costs well under a millisecond a
/// day here.
/// </para>
/// <para>
/// Health and loyalty are <b>individual</b> rather than per-estate, because
/// mortality reads health and emigration reads loyalty, and neither is a
/// question about a building.
/// </para>
/// </remarks>
public sealed class Citizens(Tables tables)
{
    private readonly Tables _t = tables;
    private readonly List<int> _id = [];
    private readonly List<int> _home = [];
    private readonly List<int> _workplace = [];
    private readonly List<int> _age = [];
    private readonly List<Commute> _commute = [];
    private readonly List<int> _schoolDays = [];
    private readonly List<bool> _studying = [];
    private readonly List<double> _health = [];
    private readonly List<double> _loyalty = [];

    private int _nextId = 1;

    /// <summary>Somebody who has just arrived: in good health, neither committed nor disaffected.</summary>
    public const double ArrivingHealth = 0.9;

    public const double ArrivingLoyalty = 0.6;

    public int Count => _id.Count;

    public int IdAt(int i) => _id[i];

    /// <summary>The building id they live in.</summary>
    public int HomeAt(int i) => _home[i];

    /// <summary>The building id they work in, or -1.</summary>
    public int WorkplaceAt(int i) => _workplace[i];

    public int AgeAt(int i) => _age[i];

    public Commute CommuteAt(int i) => _commute[i];

    /// <summary>Days of attendance, at school and then at university.</summary>
    public int SchoolDaysAt(int i) => _schoolDays[i];

    /// <summary>
    /// Enrolled at a university right now, and therefore not available for work.
    /// A student is a working-age adult who is <i>not</i> working, and that is
    /// what makes a university a cost as well as an investment.
    /// </summary>
    public bool StudyingAt(int i) => _studying[i];

    public double HealthAt(int i) => _health[i];

    /// <summary>
    /// How they feel about living here. Follows the contentment of their home
    /// slowly rather than tracking it, so one bad winter does not empty a town.
    /// </summary>
    public double LoyaltyAt(int i) => _loyalty[i];

    public void SetHome(int i, int building) => _home[i] = building;

    public void SetWorkplace(int i, int building, Commute commute)
    {
        _workplace[i] = building;
        _commute[i] = commute;
    }

    public void SetStudying(int i, bool studying) => _studying[i] = studying;

    public void AddSchoolDay(int i) => _schoolDays[i]++;

    public void SetHealth(int i, double v) => _health[i] = Math.Clamp(v, 0.0, 1.0);

    public void SetLoyalty(int i, double v) => _loyalty[i] = Math.Clamp(v, 0.0, 1.0);

    public void SetAge(int i, int years) => _age[i] = years;

    public int Add(int home, int age, int schoolDays, double health, double loyalty)
    {
        _id.Add(_nextId++);
        _home.Add(home);
        _workplace.Add(-1);
        _age.Add(age);
        _commute.Add(Commute.None);
        _schoolDays.Add(schoolDays);
        _studying.Add(false);
        _health.Add(health);
        _loyalty.Add(loyalty);
        return _id.Count - 1;
    }

    /// <summary>
    /// Put somebody back exactly as they were, keeping their id.
    /// </summary>
    /// <remarks>
    /// Only a load calls this. The id is not decoration: a birthday is derived
    /// from it, so renumbering the population on load would move everybody's
    /// birthday and quietly change when they age.
    /// </remarks>
    internal int Restore(int id, int home, int age, int schoolDays, double health, double loyalty)
    {
        var i = Add(home, age, schoolDays, health, loyalty);
        _id[i] = id;
        _nextId = Math.Max(_nextId, id + 1);
        return i;
    }

    /// <summary>Somebody arriving from outside, schooled, in good health.</summary>
    public int AddArrival(int home, int age) =>
        Add(home, age, _t.SchoolDays, ArrivingHealth, ArrivingLoyalty);

    public void RemoveAt(int i)
    {
        _id.RemoveAt(i);
        _home.RemoveAt(i);
        _workplace.RemoveAt(i);
        _age.RemoveAt(i);
        _commute.RemoveAt(i);
        _schoolDays.RemoveAt(i);
        _studying.RemoveAt(i);
        _health.RemoveAt(i);
        _loyalty.RemoveAt(i);
    }

    /// <summary>What the days of attendance add up to.</summary>
    public Education EducationAt(int i)
    {
        var days = _schoolDays[i];
        if (days >= _t.SchoolDays + _t.UniversityDays)
        {
            return Education.Graduate;
        }

        return days >= _t.SchoolDays ? Education.Schooled : Education.Unschooled;
    }

    public LifeStage StageAt(int i)
    {
        if (_studying[i])
        {
            return LifeStage.Student;
        }

        var age = _age[i];
        if (age < _t.SchoolAgeFrom)
        {
            return LifeStage.Infant;
        }

        if (age < _t.SchoolAgeTo)
        {
            return LifeStage.Pupil;
        }

        return age >= _t.WorkingAgeFrom && age < _t.WorkingAgeTo
            ? LifeStage.Worker
            : LifeStage.Retired;
    }

    /// <summary>Whether they are available to hold a job.</summary>
    public bool CanWork(int i) =>
        _age[i] >= _t.WorkingAgeFrom && _age[i] < _t.WorkingAgeTo && !_studying[i];

    /// <summary>
    /// The day of the year they were born on.
    /// </summary>
    /// <remarks>
    /// Derived from the id rather than stored, so ageing is spread evenly over
    /// the year instead of the whole republic having a birthday at once. A
    /// cohort that ages on one day is a cohort that <i>dies</i> on one day, and a
    /// population graph with that sawtooth in it is a modelling artefact a
    /// player would rightly read as a bug.
    /// </remarks>
    public int BirthdayAt(int i) => _id[i] % SimClock.DaysPerYear;

    /// <summary>The index of a citizen by id, or -1 if they are no longer here.</summary>
    public int IndexOfId(int id)
    {
        for (var i = 0; i < _id.Count; i++)
        {
            if (_id[i] == id)
            {
                return i;
            }
        }

        return -1;
    }

    public int Employed()
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (_workplace[i] >= 0)
            {
                n++;
            }
        }

        return n;
    }

    public int ByStage(LifeStage stage)
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (StageAt(i) == stage)
            {
                n++;
            }
        }

        return n;
    }

    public int ByEducation(Education level)
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (EducationAt(i) == level)
            {
                n++;
            }
        }

        return n;
    }

    public (double Health, double Loyalty) MeanWellbeing()
    {
        if (Count == 0)
        {
            return (0.0, 0.0);
        }

        var health = 0.0;
        var loyalty = 0.0;
        for (var i = 0; i < Count; i++)
        {
            health += _health[i];
            loyalty += _loyalty[i];
        }

        return (health / Count, loyalty / Count);
    }

    /// <summary>
    /// Who lives where, counted in a <b>single walk</b> of the population.
    /// </summary>
    /// <remarks>
    /// Asking each home in turn would be a walk per home, which is a republic
    /// squared. The contentment pass wants this for every home every day.
    /// </remarks>
    public Dictionary<int, HomeCensus> CensusByHome()
    {
        var census = new Dictionary<int, HomeCensus>();
        for (var i = 0; i < Count; i++)
        {
            var home = _home[i];
            census.TryGetValue(home, out var row);
            row.Residents++;
            row.Health += _health[i];
            row.Loyalty += _loyalty[i];
            if (CanWork(i))
            {
                row.WorkingAge++;
                if (_workplace[i] >= 0)
                {
                    row.Employed++;
                }
            }

            if (_age[i] < _t.SchoolAgeTo)
            {
                row.Children++;
            }

            census[home] = row;
        }

        // Means rather than sums, so a caller reads a figure per person.
        foreach (var home in census.Keys.ToList())
        {
            var row = census[home];
            if (row.Residents > 0)
            {
                row.Health /= row.Residents;
                row.Loyalty /= row.Residents;
            }

            census[home] = row;
        }

        return census;
    }

    /// <summary>How many people need a seat on a bus to hold their job.</summary>
    public int Riders()
    {
        var n = 0;
        for (var i = 0; i < Count; i++)
        {
            if (_commute[i].IsCarried)
            {
                n++;
            }
        }

        return n;
    }
}
