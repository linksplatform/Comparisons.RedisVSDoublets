using System;
using System.ComponentModel.DataAnnotations;

namespace Comparisons.RedisVSDoublets.Model
{
    public class BlogPost
    {
        [Key]
        public int Id { get; set; }

        [Required]
        public string Title { get; set; } = string.Empty;

        [Required]
        public string Content { get; set; } = string.Empty;

        [Required]
        public DateTime PublicationDateTime { get; set; }

        public override string ToString()
        {
            return $"{Id}. {Title} ({PublicationDateTime:yyyy-MM-dd HH:mm:ss}) {Content}";
        }
    }
}