namespace App.Models
{
    public class User
    {
        public string Name { get; set; }
        public int Age { get; set; }
        public string Describe() { return Name; }
    }
}
