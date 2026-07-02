using App.Models;
using App.Services;

namespace App
{
    public class Program
    {
        public static void Main()
        {
            var svc = new UserService();
            var user = svc.Create("test");
            Helper.Assist();
        }
    }
}
